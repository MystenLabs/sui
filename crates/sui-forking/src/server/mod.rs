// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context as _, Result};
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use prometheus::Registry;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use simulacrum::AdvanceEpochConfig;
use sui_data_store::{
    Node, ObjectKey, ObjectStore, VersionQuery,
    stores::{DataStore, FileSystemStore, NODE_MAPPING_FILE, ReadThroughStore},
};
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_reader::{
    consistent_reader::ConsistentReaderArgs,
    ledger_grpc_reader::{LedgerGrpcArgs, LedgerGrpcReader},
};
use sui_pg_db::{Db, DbArgs, reset_database};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::ChainIdentifier,
    effects::TransactionEffectsAPI,
    supported_protocol_versions::{
        Chain::{self},
        ProtocolConfig,
    },
    transaction::Transaction,
};

use crate::grpc::transaction_execution_service::ForkingTransactionExecutionService;
use crate::grpc::{RpcArgs as GrpcArgs, ledger_service::ForkingLedgerService};
use crate::grpc::{RpcService as GrpcRpcService, consistent_store::ForkingConsistentStore};
use crate::grpc::{TlsArgs as GrpcTlsArgs, subscription_service::ForkingSubscriptionService};
use crate::{graphql::GraphQLClient, store::ForkingStore};

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentServiceServer;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionServiceServer;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerServiceServer,
    subscription_service_server::SubscriptionServiceServer,
};
use sui_swarm_config::network_config_builder::ConfigBuilder;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdvanceClockRequest {
    pub seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteTxRequest {
    /// Base64 encoded transaction bytes
    pub tx_bytes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteTxResponse {
    /// Base64 encoded transaction effects
    pub effects: String,
    /// Execution error if any
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForkingStatus {
    pub forked_at_checkpoint: u64,
    pub checkpoint: u64,
    pub epoch: u64,
    pub transaction_count: usize,
}

/// The shared state for the forking server
pub struct AppState {
    pub context: crate::context::Context,
    pub transaction_count: Arc<AtomicUsize>,
    pub forked_at_checkpoint: u64,
    pub _chain: Chain,
    pub _protocol_config: ProtocolConfig,
}

impl AppState {
    pub async fn new(
        context: crate::context::Context,
        chain: Chain,
        forked_at_checkpoint: u64,
        protocol_config: ProtocolConfig,
    ) -> Self {
        Self {
            context,
            transaction_count: Arc::new(AtomicUsize::new(0)),
            forked_at_checkpoint,
            _chain: chain,
            _protocol_config: protocol_config,
        }
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.context.simulacrum.read().await;
    let store = sim.store();

    let checkpoint = store
        .get_highest_checkpint()
        .map(|c| c.sequence_number)
        .unwrap_or(0);

    // Get the current epoch from the checkpoint
    let epoch = store
        .get_highest_checkpint()
        .map(|c| c.epoch())
        .unwrap_or(0);

    let status = ForkingStatus {
        forked_at_checkpoint: state.forked_at_checkpoint,
        checkpoint,
        epoch,
        transaction_count: state.transaction_count.load(Ordering::SeqCst),
    };

    Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    })
}

async fn advance_checkpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    // create_checkpoint returns a VerifiedCheckpoint, not a Result
    let checkpoint = sim.create_checkpoint();
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced to checkpoint {}", checkpoint.sequence_number);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!(
            "Advanced to checkpoint {}",
            checkpoint.sequence_number
        )),
        error: None,
    })
}

async fn advance_clock(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdvanceClockRequest>,
) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    let duration = std::time::Duration::from_secs(request.seconds);
    sim.advance_clock(duration);
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced clock by {} seconds", request.seconds);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} seconds", request.seconds)),
        error: None,
    })
}

async fn advance_epoch(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    // Use default configuration for advancing epoch
    let config = AdvanceEpochConfig::default();
    sim.advance_epoch(config);
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced to next epoch");

    Json(ApiResponse::<String> {
        success: true,
        data: Some("Advanced to next epoch".to_string()),
        error: None,
    })
}

async fn execute_tx(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExecuteTxRequest>,
) -> impl IntoResponse {
    // Decode the base64 transaction bytes
    let tx_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &request.tx_bytes,
    ) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Json(ApiResponse::<ExecuteTxResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to decode base64: {}", e)),
            });
        }
    };

    // Deserialize the transaction
    let transaction: Transaction = match bcs::from_bytes(&tx_bytes) {
        Ok(tx) => tx,
        Err(e) => {
            return Json(ApiResponse::<ExecuteTxResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to deserialize transaction: {}", e)),
            });
        }
    };

    // Execute the transaction
    let mut sim = state.context.simulacrum.write().await;
    match sim.execute_transaction(transaction) {
        Ok((effects, execution_error)) => {
            let effects_bytes = bcs::to_bytes(&effects).unwrap();
            let effects_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &effects_bytes);

            let error_str = execution_error.map(|e| format!("{:?}", e));

            state.transaction_count.fetch_add(1, Ordering::SeqCst);
            info!("Executed transaction successfully");

            Json(ApiResponse {
                success: true,
                data: Some(ExecuteTxResponse {
                    effects: effects_base64,
                    error: error_str,
                }),
                error: None,
            })
        }
        Err(e) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute transaction: {}", e)),
        }),
    }
}

#[derive(serde::Deserialize)]
struct FaucetRequest {
    address: SuiAddress,
    amount: u64,
}

async fn faucet(
    State(state): State<Arc<AppState>>,
    Json(request): Json<FaucetRequest>,
) -> impl IntoResponse {
    let FaucetRequest { address, amount } = request;

    let mut simulacrum = state.context.simulacrum.write().await;
    let response = simulacrum.request_gas(address, amount);

    match response {
        Ok(effects) => {
            println!("Effects {:?}", effects.created());
            let effects_bytes = bcs::to_bytes(&effects).unwrap();
            let effects_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &effects_bytes);

            state.transaction_count.fetch_add(1, Ordering::SeqCst);
            info!("Executed transaction successfully");

            Json(ApiResponse {
                success: true,
                data: Some(ExecuteTxResponse {
                    effects: effects_base64,
                    error: None,
                }),
                error: None,
            })
        }
        Err(e) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute transaction: {}", e)),
        }),
    }
}

/// Start the forking server
pub async fn start_server(
    // accounts: InitialAccounts,
    chain: Chain,
    checkpoint: Option<u64>,
    host: String,
    port: u16,
    data_ingestion_path: PathBuf,
    version: &'static str,
) -> Result<()> {
    let chain_str = chain.as_str();
    let client = GraphQLClient::new(format!("https://graphql.{chain_str}.sui.io/graphql"));
    let (at_checkpoint, protocol_version) = if let Some(cp) = checkpoint {
        (cp, client.fetch_protocol_version(Some(cp)).await?)
    } else {
        client
            .fetch_latest_checkpoint_and_protocol_version()
            .await?
    };
    println!(
        "Starting at checkpoint {} with protocol version {}",
        at_checkpoint, protocol_version
    );
    let protocol_config = ProtocolConfig::get_for_version(protocol_version.into(), chain);
    let database_url = Url::parse("postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt")?;

    {
        reset_database(database_url.clone(), DbArgs::default(), None).await?;
    }

    let node = match chain {
        Chain::Mainnet => Node::Mainnet,
        Chain::Testnet => Node::Testnet,
        Chain::Unknown => todo!("Add support for custom chains"),
    };

    let forking_path = format!(
        "forking/{}/forked_at_checkpoint_{}",
        chain.as_str(),
        at_checkpoint
    );

    let fs_base_path = FileSystemStore::base_path().unwrap().join(forking_path);
    let fs = FileSystemStore::new_with_path(node.clone(), fs_base_path.clone()).unwrap();
    let gql_rpc_store = DataStore::new(node.clone(), version).unwrap();
    let fs_transaction_store =
        FileSystemStore::new_with_path(node.clone(), fs_base_path.clone()).unwrap();
    let object_store = ReadThroughStore::new(fs, gql_rpc_store);

    info!("Fs base path {:?}", fs_base_path.display());
    let node_mapping_file = fs_base_path.join(NODE_MAPPING_FILE);
    if !fs_base_path.exists() {
        std::fs::create_dir_all(fs_base_path).unwrap();
    }
    info!("Node mapping file path: {:?}", node_mapping_file.display());
    if !node_mapping_file.exists() {
        std::fs::write(
            node_mapping_file,
            format!("{},{}", node.network_name(), chain_str),
        )
        .unwrap();
    }

    let mut rng = rand::rngs::OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build();

    let store = ForkingStore::new(
        &config.genesis,
        at_checkpoint,
        fs_transaction_store,
        object_store,
    );
    let mut simulacrum = simulacrum::Simulacrum::new_with_network_config_store(&config, rng, store);
    simulacrum.set_data_ingestion_path(data_ingestion_path.clone());
    println!("Data ingestion path: {:?}", data_ingestion_path);

    let simulacrum = Arc::new(RwLock::new(simulacrum));

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")
        .unwrap();

    let metrics_args = sui_indexer_alt_metrics::MetricsArgs::default();
    let metrics = MetricsService::new(metrics_args, registry.clone());
    let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("RPC listening on {}", rpc_listen_address);

    let grpc_args = GrpcArgs {
        rpc_listen_address,
        tls: GrpcTlsArgs::default(),
    };

    let context = crate::context::Context {
        simulacrum: simulacrum.clone(),
        at_checkpoint,
        chain,
        protocol_version,
    };

    let subscription_service = ForkingSubscriptionService::new(context.clone());
    let consistent_store = ForkingConsistentStore::new(context.clone());
    let ledger_service = ForkingLedgerService::new(simulacrum.clone(), ChainIdentifier::random());
    let tx_execution_service = ForkingTransactionExecutionService::new(context.clone());
    let grpc = GrpcRpcService::new(grpc_args, version, &registry)
        .await?
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(
            sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::FILE_DESCRIPTOR_SET,
        )
        .add_service(ConsistentServiceServer::new(consistent_store))
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(SubscriptionServiceServer::new(subscription_service))
        .add_service(TransactionExecutionServiceServer::new(tx_execution_service));
    let grpc_service = grpc.run().await?;

    let state =
        Arc::new(AppState::new(context.clone(), chain, at_checkpoint, protocol_config).await);

    let update_objects_handle = tokio::spawn(async move {
        update_system_objects(context.clone()).await.unwrap();
    });

    println!("Ready to accept requests");

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(get_status))
        .route("/advance-checkpoint", post(advance_checkpoint))
        .route("/advance-clock", post(advance_clock))
        .route("/advance-epoch", post(advance_epoch))
        .route("/execute-tx", post(execute_tx))
        .route("/faucet", post(faucet))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("Forking server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Set up graceful shutdown handler
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("\nReceived CTRL+C, shutting down gracefully...");
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Abort the spawned tasks when the server shuts down
    // rpc_handle.abort();
    // indexer_handle.abort();
    update_objects_handle.abort();

    info!("Server shutdown complete");

    Ok(())
}

const SYSTEM_OBJECT_IDS: &[&str] = &["0x1", "0x2", "0x3", "0x5", "0x6"];

/// Update system objects to the versions at the forking checkpoint
async fn update_system_objects(context: crate::context::Context) -> anyhow::Result<()> {
    let crate::context::Context { at_checkpoint, .. } = context;

    info!(
        "Fetching system objects from RPC at checkpoint {}",
        at_checkpoint
    );
    let object_ids: Vec<ObjectID> = SYSTEM_OBJECT_IDS
        .iter()
        .map(|id| ObjectID::from_hex_literal(id).unwrap())
        .collect();

    let keys: Vec<ObjectKey> = object_ids
        .iter()
        .map(|&object_id| ObjectKey {
            object_id,
            version_query: VersionQuery::AtCheckpoint(at_checkpoint),
        })
        .collect();

    let simulacrum = context.simulacrum.write().await;
    let data_store = simulacrum.store_static();
    data_store.object_store().get_objects(&keys).unwrap();

    Ok(())
}

// /// Start the indexers: both the main indexer and the consistent store
// async fn start_indexers(data_ingestion_path: PathBuf, version: &'static str) -> Result<()> {
//     let registry = prometheus::Registry::new();
//     let rocksdb_db_path = tempdir().unwrap().keep();
//     let db_url_str = "postgres://postgres:postgrespw@localhost:5432";
//     let db_url = Url::parse(&format!("{db_url_str}/sui_indexer_alt")).unwrap();
//     // drop_and_recreate_db(db_url_str).unwrap();
//     let indexer_config = IndexerConfig::new(db_url, data_ingestion_path.clone());
//     let consistent_store_config = ConsistentStoreConfig::new(
//         rocksdb_db_path.clone(),
//         indexer_config.indexer_args.clone(),
//         indexer_config.client_args.clone(),
//     );
//     let indexer = start_indexer(indexer_config, &registry).await?;
//     let consistent_store =
//         start_consistent_store(consistent_store_config, &registry, version).await?;
//
//     match indexer.attach(consistent_store).main().await {
//         Ok(()) | Err(sui_futures::service::Error::Terminated) => {}
//
//         Err(sui_futures::service::Error::Aborted) => {
//             std::process::exit(1);
//         }
//
//         Err(sui_futures::service::Error::Task(_)) => {
//             std::process::exit(2);
//         }
//     }
//
//     Ok(())
// }

// fn drop_and_recreate_db(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
//     // Connect to the 'postgres' database (not your target database)
//     let mut conn = PgConnection::establish(db_url)?;
//
//     info!("Dropping and recreating database sui_indexer_alt...");
//     // // Drop the database
//     diesel::sql_query("DROP DATABASE IF EXISTS sui_indexer_alt").execute(&mut conn)?;
//
//     // Recreate it
//     diesel::sql_query("CREATE DATABASE sui_indexer_alt").execute(&mut conn)?;
//
//     Ok(())
// }

// /// Insert an object's version info into the obj_versions table
// pub(crate) async fn insert_obj_version_into_db(
//     db_writer: &sui_pg_db::Db,
//     object: &sui_types::object::Object,
//     cp_sequence_number: i64,
// ) -> anyhow::Result<()> {
//     use diesel::prelude::*;
//     use sui_indexer_alt_schema::schema::obj_versions;
//
//     let object_id = object.id().to_vec();
//     let object_version = object.version().value() as i64;
//     let object_digest = Some(object.digest().into_inner().to_vec());
//
//     let mut conn = db_writer.connect().await?;
//
//     diesel_async::RunQueryDsl::execute(
//         diesel::insert_into(obj_versions::table)
//             .values((
//                 obj_versions::object_id.eq(&object_id),
//                 obj_versions::object_version.eq(object_version),
//                 obj_versions::object_digest.eq(&object_digest),
//                 obj_versions::cp_sequence_number.eq(cp_sequence_number),
//             ))
//             .on_conflict((obj_versions::object_id, obj_versions::object_version))
//             .do_nothing(),
//         &mut conn,
//     )
//     .await?;
//
//     info!(
//         "Inserted obj_version for {} version {} into obj_versions table",
//         object.id(),
//         object_version
//     );
//
//     Ok(())
// }

// /// Insert an object into the kv_objects table
// pub(crate) async fn insert_kv_object_into_db(
//     db_writer: &sui_pg_db::Db,
//     object: &sui_types::object::Object,
// ) -> anyhow::Result<()> {
//     use diesel::prelude::*;
//     use sui_indexer_alt_schema::schema::kv_objects;
//
//     let object_id = object.id().to_vec();
//     let object_version = object.version().value() as i64;
//     let serialized_object = Some(bcs::to_bytes(object)?);
//
//     let mut conn = db_writer.connect().await?;
//
//     diesel_async::RunQueryDsl::execute(
//         diesel::insert_into(kv_objects::table)
//             .values((
//                 kv_objects::object_id.eq(&object_id),
//                 kv_objects::object_version.eq(object_version),
//                 kv_objects::serialized_object.eq(&serialized_object),
//             ))
//             .on_conflict((kv_objects::object_id, kv_objects::object_version))
//             .do_nothing(),
//         &mut conn,
//     )
//     .await?;
//
//     info!(
//         "Inserted object {} version {} into kv_objects table",
//         object.id(),
//         object_version
//     );
//
//     Ok(())
// }

// /// Insert an object's info into the obj_info table
// pub(crate) async fn insert_obj_info_into_db(
//     db_writer: &sui_pg_db::Db,
//     object: &sui_types::object::Object,
//     cp_sequence_number: i64,
// ) -> anyhow::Result<()> {
//     use diesel::prelude::*;
//     use sui_indexer_alt_schema::objects::StoredOwnerKind;
//     use sui_indexer_alt_schema::schema::obj_info;
//     use sui_types::object::Owner;
//
//     let object_id = object.id().to_vec();
//
//     let (owner_kind, owner_id) = match object.owner() {
//         Owner::AddressOwner(a) => (Some(StoredOwnerKind::Address), Some(a.to_vec())),
//         Owner::ObjectOwner(o) => (Some(StoredOwnerKind::Object), Some(o.to_vec())),
//         Owner::Shared { .. } => (Some(StoredOwnerKind::Shared), None),
//         Owner::Immutable => (Some(StoredOwnerKind::Immutable), None),
//         Owner::ConsensusAddressOwner { owner, .. } => {
//             (Some(StoredOwnerKind::Address), Some(owner.to_vec()))
//         }
//     };
//
//     let type_ = object.struct_tag();
//     let package: Option<Vec<u8>> = type_.as_ref().map(|t| t.address.to_vec());
//     let module: Option<String> = type_.as_ref().map(|t| t.module.to_string());
//     let name: Option<String> = type_.as_ref().map(|t| t.name.to_string());
//     let instantiation: Option<Vec<u8>> = type_
//         .as_ref()
//         .map(|t| bcs::to_bytes(&t.type_params))
//         .transpose()?;
//
//     let mut conn = db_writer.connect().await?;
//
//     diesel_async::RunQueryDsl::execute(
//         diesel::insert_into(obj_info::table)
//             .values((
//                 obj_info::object_id.eq(&object_id),
//                 obj_info::cp_sequence_number.eq(cp_sequence_number),
//                 obj_info::owner_kind.eq(owner_kind),
//                 obj_info::owner_id.eq(&owner_id),
//                 obj_info::package.eq(&package),
//                 obj_info::module.eq(&module),
//                 obj_info::name.eq(&name),
//                 obj_info::instantiation.eq(&instantiation),
//             ))
//             .on_conflict((obj_info::object_id, obj_info::cp_sequence_number))
//             .do_nothing(),
//         &mut conn,
//     )
//     .await?;
//
//     info!("Inserted obj_info for {} into obj_info table", object.id(),);
//
//     Ok(())
// }
