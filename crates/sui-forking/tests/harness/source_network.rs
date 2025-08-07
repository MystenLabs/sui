// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use fastcrypto::encoding::{Base58, Encoding};
use serde::Deserialize;
use serde_json::json;
use sui_rpc_api::Client as RpcClient;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::object::Object;
use sui_types::sui_system_state::SuiSystemStateTrait;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::sync::{RwLock, oneshot};
use tokio::time::sleep;
use url::Url;

use crate::harness::ports;

const SOURCE_ACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMode {
    Fast,
    FullStack,
}

pub struct SourceNetworkHarness {
    test_cluster: TestCluster,
    graphql_server: MockGraphQlServer,
    graphql_url: Url,
    fullnode_url: Url,
    fork_checkpoint: u64,
    seed_address: SuiAddress,
    seed_object_ids: Vec<ObjectID>,
}

impl SourceNetworkHarness {
    pub async fn fast() -> Result<Self> {
        Self::new(SourceMode::Fast).await
    }

    pub async fn full_stack() -> Result<Self> {
        Self::new(SourceMode::FullStack).await
    }

    pub fn graphql_url(&self) -> &Url {
        &self.graphql_url
    }

    pub fn fullnode_url(&self) -> &Url {
        &self.fullnode_url
    }

    pub fn fork_checkpoint(&self) -> u64 {
        self.fork_checkpoint
    }

    pub fn seed_address(&self) -> SuiAddress {
        self.seed_address
    }

    pub fn seed_object_ids(&self) -> &[ObjectID] {
        &self.seed_object_ids
    }

    pub async fn produce_source_activity(&mut self, tx_count: usize) -> Result<u64> {
        if tx_count == 0 {
            return self.latest_checkpoint().await;
        }

        let mut latest_checkpoint = self.latest_checkpoint().await?;
        for _ in 0..tx_count {
            latest_checkpoint = produce_checkpoint(&self.test_cluster, latest_checkpoint).await?;
        }

        self.graphql_server
            .set_latest_checkpoint(latest_checkpoint)
            .await;

        Ok(latest_checkpoint)
    }

    async fn new(mode: SourceMode) -> Result<Self> {
        let test_cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            .build()
            .await;

        let fullnode_url = Url::parse(test_cluster.rpc_url()).with_context(|| {
            format!("invalid test cluster rpc url '{}'", test_cluster.rpc_url())
        })?;

        let seed_address = test_cluster.get_address_0();
        let seed_object_ids = fetch_owned_object_ids(&test_cluster, seed_address).await?;

        let latest_checkpoint = latest_checkpoint(&test_cluster).await?;
        let protocol_version = test_cluster.get_sui_system_state().protocol_version();
        let chain_identifier_base58 =
            Base58::encode(test_cluster.get_chain_identifier().as_bytes());

        let mut owned_objects_by_address = HashMap::new();
        owned_objects_by_address.insert(seed_address.to_string(), seed_object_ids.clone());

        let graphql_state = MockGraphQlState {
            latest_checkpoint,
            protocol_version,
            chain_identifier_base58,
            owned_objects_by_address,
            fullnode_url: fullnode_url.clone(),
        };

        let graphql_server = MockGraphQlServer::start(graphql_state).await?;
        let graphql_url = graphql_server.url().clone();

        let mut harness = Self {
            test_cluster,
            graphql_server,
            graphql_url,
            fullnode_url,
            fork_checkpoint: latest_checkpoint,
            seed_address,
            seed_object_ids,
        };

        if mode == SourceMode::FullStack {
            harness.produce_source_activity(2).await?;
            harness.fork_checkpoint = harness.latest_checkpoint().await?;
        }

        Ok(harness)
    }

    async fn latest_checkpoint(&self) -> Result<u64> {
        latest_checkpoint(&self.test_cluster).await
    }
}

async fn latest_checkpoint(test_cluster: &TestCluster) -> Result<u64> {
    let mut client = test_cluster.grpc_client();
    let checkpoint = client
        .get_latest_checkpoint()
        .await
        .context("failed to read latest checkpoint from source network")?;
    Ok(*checkpoint.data().sequence_number())
}

async fn fetch_owned_object_ids(
    test_cluster: &TestCluster,
    owner: SuiAddress,
) -> Result<Vec<ObjectID>> {
    let client = test_cluster.grpc_client();
    let page = client
        .get_owned_objects(owner, None, Some(128), None)
        .await
        .with_context(|| format!("failed to fetch source owned objects for {owner}"))?;

    Ok(page.items.into_iter().map(|object| object.id()).collect())
}

async fn produce_checkpoint(test_cluster: &TestCluster, previous_checkpoint: u64) -> Result<u64> {
    let recipient = SuiAddress::random_for_testing_only();
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .transfer_sui(Some(1), recipient)
        .build();
    let _ = test_cluster.sign_and_execute_transaction(&tx_data).await;

    let deadline = Instant::now() + SOURCE_ACTIVITY_TIMEOUT;
    loop {
        let latest = latest_checkpoint(test_cluster).await?;
        if latest > previous_checkpoint {
            return Ok(latest);
        }

        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for source checkpoint to advance beyond {}",
                previous_checkpoint
            );
        }

        sleep(Duration::from_millis(200)).await;
    }
}

struct MockGraphQlServer {
    state: std::sync::Arc<RwLock<MockGraphQlState>>,
    url: Url,
    shutdown_sender: Option<oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockGraphQlServer {
    async fn start(state: MockGraphQlState) -> Result<Self> {
        let graphql_port = ports::allocate_ports(1)
            .context("failed to allocate mock GraphQL port")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("failed to allocate mock GraphQL port"))?;

        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, graphql_port));
        let state = std::sync::Arc::new(RwLock::new(state));

        let listener = std::net::TcpListener::bind(address)
            .with_context(|| format!("failed to bind mock GraphQL listener on {address}"))?;
        listener
            .set_nonblocking(true)
            .context("failed to set mock GraphQL listener non-blocking mode")?;

        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let thread_state = state.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("mock GraphQL runtime");

            runtime.block_on(async move {
                let app = Router::new()
                    .route("/graphql", post(handle_graphql_request))
                    .with_state(thread_state);

                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("mock GraphQL tokio listener");

                if let Err(error) = axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_receiver.await;
                    })
                    .await
                {
                    eprintln!("mock GraphQL server exited with error: {error}");
                }
            });
        });

        let url = Url::parse(&format!("http://{address}/graphql"))
            .with_context(|| format!("failed to build mock GraphQL url for {address}"))?;

        Ok(Self {
            state,
            url,
            shutdown_sender: Some(shutdown_sender),
            thread: Some(thread),
        })
    }

    fn url(&self) -> &Url {
        &self.url
    }

    async fn set_latest_checkpoint(&self, checkpoint: u64) {
        self.state.write().await.latest_checkpoint = checkpoint;
    }
}

impl Drop for MockGraphQlServer {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }

        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone, Debug)]
struct MockGraphQlState {
    latest_checkpoint: u64,
    protocol_version: u64,
    chain_identifier_base58: String,
    owned_objects_by_address: HashMap<String, Vec<ObjectID>>,
    fullnode_url: Url,
}

#[derive(Debug, Deserialize)]
struct GraphQlRequest {
    query: String,
    variables: Option<serde_json::Value>,
}

async fn handle_graphql_request(
    State(state): State<std::sync::Arc<RwLock<MockGraphQlState>>>,
    Json(request): Json<GraphQlRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let normalized_query: String = request
        .query
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();

    if normalized_query.contains("chainIdentifier") {
        let chain_identifier_base58 = state.read().await.chain_identifier_base58.clone();
        return (
            StatusCode::OK,
            Json(json!({ "data": { "chainIdentifier": chain_identifier_base58 } })),
        );
    }

    if normalized_query.contains("checkpoint(sequenceNumber:") {
        let protocol_version = state.read().await.protocol_version;
        return (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "checkpoint": {
                        "query": {
                            "protocolConfigs": {
                                "protocolVersion": protocol_version
                            }
                        }
                    }
                }
            })),
        );
    }

    if normalized_query.contains("checkpoint{") && normalized_query.contains("sequenceNumber") {
        let state = state.read().await;
        return (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "checkpoint": {
                        "sequenceNumber": state.latest_checkpoint,
                        "query": {
                            "protocolConfigs": {
                                "protocolVersion": state.protocol_version
                            }
                        }
                    }
                }
            })),
        );
    }

    if normalized_query.contains("address(") && normalized_query.contains("objects(") {
        return handle_owned_objects_query(state, request.variables).await;
    }

    if normalized_query.contains("multiGetObjects(") {
        return handle_multi_get_objects_query(state, request.variables).await;
    }

    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "errors": [
                {
                    "message": format!("unrecognized mock graphql query: {}", request.query)
                }
            ]
        })),
    )
}

async fn handle_owned_objects_query(
    state: std::sync::Arc<RwLock<MockGraphQlState>>,
    variables: Option<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(address) = variables
        .as_ref()
        .and_then(|value| value.get("address"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "errors": [{ "message": "missing 'address' variable in mock graphql request" }]
            })),
        );
    };

    let has_after_cursor = variables
        .as_ref()
        .and_then(|value| value.get("after"))
        .map(|value| !value.is_null())
        .unwrap_or(false);

    let edges = if has_after_cursor {
        Vec::new()
    } else {
        state
            .read()
            .await
            .owned_objects_by_address
            .get(&address)
            .map(|object_ids| {
                object_ids
                    .iter()
                    .map(|object_id| json!({ "node": { "address": object_id.to_string() } }))
                    .collect()
            })
            .unwrap_or_default()
    };

    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "address": {
                    "objects": {
                        "edges": edges,
                        "pageInfo": {
                            "endCursor": null,
                            "hasNextPage": false
                        }
                    }
                }
            }
        })),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultiGetObjectKeyVariable {
    address: String,
    version: Option<u64>,
    root_version: Option<u64>,
    at_checkpoint: Option<u64>,
}

async fn handle_multi_get_objects_query(
    state: std::sync::Arc<RwLock<MockGraphQlState>>,
    variables: Option<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(keys) = variables
        .as_ref()
        .and_then(|value| value.get("keys"))
        .cloned()
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "errors": [{ "message": "missing 'keys' variable in multiGetObjects request" }]
            })),
        );
    };

    let keys: Vec<MultiGetObjectKeyVariable> = match serde_json::from_value(keys) {
        Ok(keys) => keys,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "errors": [{ "message": format!("invalid 'keys' payload for multiGetObjects: {error}") }]
                })),
            );
        }
    };

    let fullnode_url = state.read().await.fullnode_url.clone();
    let mut client = match RpcClient::new(fullnode_url.as_str()) {
        Ok(client) => client,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "errors": [{ "message": format!("failed to create source rpc client for {}: {error}", fullnode_url) }]
                })),
            );
        }
    };

    let mut objects = Vec::with_capacity(keys.len());
    for key in keys {
        // The mock supports the object-key flavors used by sui-forking startup paths.
        let _ = key.root_version;
        let _ = key.at_checkpoint;

        let object_id = match ObjectID::from_hex_literal(&key.address) {
            Ok(object_id) => object_id,
            Err(error) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "errors": [{ "message": format!("invalid object key address '{}': {error}", key.address) }]
                    })),
                );
            }
        };

        match fetch_object_for_key(&mut client, object_id, key.version).await {
            Ok(Some(object)) => {
                let version = object.version().value();
                let object_bcs = match bcs::to_bytes(&object) {
                    Ok(bytes) => fastcrypto::encoding::Base64::encode(bytes),
                    Err(error) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(json!({
                                "errors": [{ "message": format!("failed to serialize object {} to BCS: {error}", object_id) }]
                            })),
                        );
                    }
                };
                objects.push(Some(json!({
                    "address": object_id.to_string(),
                    "version": version,
                    "objectBcs": object_bcs
                })));
            }
            Ok(None) => objects.push(None),
            Err(error) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "errors": [{ "message": format!("failed to fetch object {} for multiGetObjects: {error}", object_id) }]
                    })),
                );
            }
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "multiGetObjects": objects,
            }
        })),
    )
}

async fn fetch_object_for_key(
    client: &mut RpcClient,
    object_id: ObjectID,
    version: Option<u64>,
) -> Result<Option<Object>> {
    let result = if let Some(version) = version {
        client
            .get_object_with_version(object_id, SequenceNumber::from_u64(version))
            .await
    } else {
        client.get_object(object_id).await
    };

    match result {
        Ok(object) => Ok(Some(object)),
        Err(error) if error.code() == tonic::Code::NotFound => Ok(None),
        Err(error) => Err(anyhow!(error)),
    }
}
