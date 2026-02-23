// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod bootstrap;
mod data_store;
mod handlers;

use std::{net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::{Context as _, Result};
use axum::{
    Router,
    routing::{get, post},
};
use prometheus::Registry;
use tokio::sync::{RwLock, oneshot};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use sui_futures::service::Service;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerServiceServer,
    subscription_service_server::SubscriptionServiceServer,
};
use sui_rpc::proto::sui::rpc::v2::{
    state_service_server::StateServiceServer,
    transaction_execution_service_server::TransactionExecutionServiceServer,
};
use sui_rpc_api::subscription::SubscriptionService as RpcSubscriptionService;
use sui_types::{
    digests::{
        ChainIdentifier, CheckpointDigest, get_mainnet_chain_identifier,
        get_testnet_chain_identifier,
    },
    supported_protocol_versions::Chain,
};

use crate::grpc::{
    RpcArgs as GrpcArgs, RpcService as GrpcRpcService, TlsArgs as GrpcTlsArgs,
    ledger_service::ForkingLedgerService, state_service::ForkingStateService,
    subscription_service::ForkingSubscriptionService,
    transaction_execution_service::ForkingTransactionExecutionService,
};
use crate::{graphql::GraphQLClient, network::ForkNetwork, seeds::StartupSeeds};

use self::bootstrap::{InitializedSimulacrum, initialize_simulacrum};
use self::data_store::{determine_startup_checkpoint, initialize_data_store};
use self::handlers::{
    AppState, advance_checkpoint, advance_clock, advance_epoch, faucet, get_status, health,
};

/// Start the forking server
pub(crate) async fn start_server(
    startup_seeds: StartupSeeds,
    fork_network: ForkNetwork,
    checkpoint: Option<u64>,
    fullnode_url: Option<String>,
    host: String,
    server_port: u16,
    rpc_port: u16,
    data_ingestion_path: PathBuf,
    version: &'static str,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<()>>,
) -> Result<()> {
    let fullnode_endpoint = fork_network
        .resolve_fullnode_endpoint(fullnode_url.as_deref())
        .context("failed to resolve fullnode RPC endpoint")?;
    let client = GraphQLClient::new(fork_network.gql_endpoint().to_string());
    let (forked_at_checkpoint, protocol_version) = if let Some(cp) = checkpoint {
        (cp, client.fetch_protocol_version(Some(cp)).await?)
    } else {
        client
            .fetch_latest_checkpoint_and_protocol_version()
            .await?
    };
    let (fs_store, fs_gql_store) = initialize_data_store(
        &fork_network,
        &fullnode_endpoint,
        forked_at_checkpoint,
        &data_ingestion_path,
        version,
    )?;
    let startup_checkpoint =
        determine_startup_checkpoint(checkpoint, forked_at_checkpoint, &fs_store)?;

    let is_resuming_existing_fork =
        checkpoint.is_some() && startup_checkpoint != forked_at_checkpoint;
    if is_resuming_existing_fork {
        println!(
            "Resuming {} forked network, current checkpoint: {}, forked at checkpoint: {}, protocol version {}",
            fork_network.display_name(),
            startup_checkpoint,
            forked_at_checkpoint,
            protocol_version
        );
    } else {
        println!(
            "Starting from {} at checkpoint {} with protocol version {}",
            fork_network.display_name(),
            forked_at_checkpoint,
            protocol_version
        );
    }

    let InitializedSimulacrum {
        simulacrum,
        faucet_owner,
    } = initialize_simulacrum(
        forked_at_checkpoint,
        startup_checkpoint,
        &client,
        &startup_seeds,
        protocol_version,
        fork_network.protocol_chain(),
        fs_store,
        fs_gql_store,
    )
    .await?;
    let simulacrum = Arc::new(RwLock::new(simulacrum));

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")?;
    let (checkpoint_sender, subscription_service_handle) = RpcSubscriptionService::build(&registry);

    let chain_id = resolve_chain_identifier(&fork_network, &client).await?;
    let context = crate::context::Context {
        simulacrum: simulacrum.clone(),
        subscription_service_handle,
        checkpoint_sender,
        chain_id,
        faucet_owner,
    };

    let grpc = start_grpc_services(context.clone(), version, &registry, rpc_port).await?;
    let grpc_handle = tokio::spawn(grpc.main());

    let state = Arc::new(AppState::new(context.clone()).await);

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(get_status))
        .route("/advance-checkpoint", post(advance_checkpoint))
        .route("/advance-clock", post(advance_clock))
        .route("/advance-epoch", post(advance_epoch))
        .route("/faucet", post(faucet))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, server_port).parse()?;
    println!("Forking server listening on {}", addr);
    println!("Ready to accept requests");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(());
    }

    let shutdown_signal = async move {
        match shutdown_receiver {
            Some(receiver) => {
                let ctrl_c = async {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            info!("\nReceived CTRL+C, shutting down gracefully...");
                        }
                        Err(err) => {
                            warn!("Failed to install CTRL+C signal handler: {err}");
                            std::future::pending::<()>().await;
                        }
                    }
                };

                tokio::select! {
                    _ = receiver => {
                        info!("received programmatic shutdown signal");
                    }
                    _ = ctrl_c => {
                    }
                }
            }
            None => {
                if let Err(err) = tokio::signal::ctrl_c().await {
                    warn!("Failed to install CTRL+C signal handler: {err}");
                    return;
                }
                info!("\nReceived CTRL+C, shutting down gracefully...");
            }
        }
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Abort the spawned tasks when the server shuts down
    grpc_handle.abort();

    info!("Server shutdown complete");

    Ok(())
}

async fn resolve_chain_identifier(
    fork_network: &ForkNetwork,
    client: &GraphQLClient,
) -> Result<ChainIdentifier> {
    match fork_network.protocol_chain() {
        Chain::Mainnet => Ok(get_mainnet_chain_identifier()),
        Chain::Testnet => Ok(get_testnet_chain_identifier()),
        Chain::Unknown => {
            let chain_identifier = client.fetch_chain_identifier().await?;
            let digest = CheckpointDigest::from_str(&chain_identifier).with_context(|| {
                format!(
                    "invalid chainIdentifier '{}' returned by {}",
                    chain_identifier,
                    client.endpoint()
                )
            })?;
            Ok(ChainIdentifier::from(digest))
        }
    }
}

/// Start the gRPC services for the forking server
async fn start_grpc_services(
    context: crate::context::Context,
    version: &'static str,
    registry: &Registry,
    rpc_port: u16,
) -> Result<Service, anyhow::Error> {
    let grpc_listen_address = SocketAddr::from(([127, 0, 0, 1], rpc_port));
    println!("RPC listening on {}", grpc_listen_address);

    let grpc_args = GrpcArgs {
        rpc_listen_address: grpc_listen_address,
        tls: GrpcTlsArgs::default(),
    };

    let subscription_service = ForkingSubscriptionService::new(context.clone());
    let ledger_service = ForkingLedgerService::new(context.clone());
    let state_service = ForkingStateService::new(context.clone());
    let tx_execution_service = ForkingTransactionExecutionService::new(context.clone());
    let grpc = GrpcRpcService::new(grpc_args, version, registry)
        .await?
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(SubscriptionServiceServer::new(subscription_service))
        .add_service(StateServiceServer::new(state_service))
        .add_service(TransactionExecutionServiceServer::new(tx_execution_service));
    let handle = grpc.run().await?;
    Ok(handle)
}
