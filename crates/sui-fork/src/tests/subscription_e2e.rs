// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the checkpoint subscription gRPC. Spins up the full
//! tonic stack (forking admin RPCs + the canonical sui-rpc-api streaming
//! RPC), drives checkpoint-producing admin calls, and asserts subscribers
//! see each checkpoint on the stream.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_protocol_config::Chain;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_api::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc_api::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_rpc_api::subscription::SubscriptionService;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use sui_types::storage::RpcStateReader;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::context::Context;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::store::DataStore;

/// In-process gRPC harness: builds a fresh Simulacrum from a genesis
/// `NetworkConfig`, wires up the subscription broker, and starts a tonic
/// server on an ephemeral port. The server task is aborted when the
/// harness is dropped.
struct ServerHarness {
    server_task: tokio::task::JoinHandle<()>,
    grpc_endpoint: String,
    // Held to keep the on-disk store alive for the lifetime of the server.
    _temp: tempfile::TempDir,
}

impl ServerHarness {
    async fn start() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        let mut data_store = DataStore::new_for_testing(temp.path().to_path_buf());
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        data_store.update_objects(written, vec![]);
        data_store.insert_checkpoint(config.genesis.checkpoint());
        data_store.insert_checkpoint_contents(config.genesis.checkpoint_contents().clone());

        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            config.genesis.checkpoint(),
            config.genesis.sui_system_object(),
            &config,
            data_store.clone(),
            rng,
        );

        let registry = Registry::new();
        let (checkpoint_sender, subscription_handle) = SubscriptionService::build(&registry);

        let context = Arc::new(Context::new(sim, Chain::Unknown, checkpoint_sender));

        let reader: Arc<dyn RpcStateReader> = Arc::new(data_store);
        let mut service = RpcService::new(reader);
        service.with_server_version(ServerVersion::new("sui-fork", "test"));
        service.with_subscription_service(subscription_handle);
        service.with_executor(Arc::new(ForkedTransactionExecutor::new(context.clone())));
        service.with_custom_service(ForkingServiceServer::new(ForkingServiceImpl::new(
            context.clone(),
        )));
        service.with_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET);

        // Bind to ephemeral port via a probe listener, then drop and let
        // `start_service` rebind. The window between is short enough not to
        // matter for in-process tests.
        let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = probe.local_addr()?;
        drop(probe);

        let server_task = tokio::spawn(async move { service.start_service(addr).await });

        let grpc_endpoint = format!("http://{addr}");

        // Wait for the server to come up by polling a connect.
        for _ in 0..50 {
            if ForkingServiceClient::connect(grpc_endpoint.clone())
                .await
                .is_ok()
            {
                return Ok(Self {
                    server_task,
                    grpc_endpoint,
                    _temp: temp,
                });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(anyhow!("timed out waiting for gRPC server to bind"))
    }
}

impl Drop for ServerHarness {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

const STREAM_RECV_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn subscription_streams_checkpoints_after_advance() -> Result<()> {
    let harness = ServerHarness::start().await?;

    let mut subscriptions =
        SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let mut stream = subscriptions
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default())
        .await?
        .into_inner();

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;

    let mut expected = Vec::with_capacity(3);
    for _ in 0..3 {
        let resp = forking
            .advance_checkpoint(AdvanceCheckpointRequest {})
            .await?
            .into_inner();
        expected.push(resp.checkpoint_sequence_number);
    }

    for expected_seq in expected {
        let msg = tokio::time::timeout(STREAM_RECV_TIMEOUT, stream.message())
            .await?
            .map_err(|e| anyhow!("stream error: {e}"))?
            .ok_or_else(|| anyhow!("subscription stream closed before advance"))?;
        let cursor = msg
            .cursor
            .ok_or_else(|| anyhow!("missing cursor on subscription message"))?;
        assert_eq!(cursor, expected_seq);
        assert!(
            msg.checkpoint.is_some(),
            "subscription message missing checkpoint payload"
        );
    }

    Ok(())
}

#[tokio::test]
async fn subscription_streams_checkpoint_after_advance_clock() -> Result<()> {
    let harness = ServerHarness::start().await?;

    let mut subscriptions =
        SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let mut stream = subscriptions
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default())
        .await?
        .into_inner();

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let clock = forking
        .advance_clock(AdvanceClockRequest {
            duration_ms: Some(1_000),
        })
        .await?
        .into_inner();
    assert!(
        !clock.tx_digest.is_empty(),
        "advance_clock should return the clock transaction digest",
    );

    let status = forking.get_status(GetStatusRequest {}).await?.into_inner();

    let msg = tokio::time::timeout(STREAM_RECV_TIMEOUT, stream.message())
        .await?
        .map_err(|e| anyhow!("stream error: {e}"))?
        .ok_or_else(|| anyhow!("subscription stream closed before advance_clock"))?;

    assert_eq!(msg.cursor, Some(status.checkpoint_sequence_number));
    assert!(
        msg.checkpoint.is_some(),
        "subscription message missing checkpoint payload"
    );

    Ok(())
}

#[tokio::test]
async fn subscription_fans_out_to_multiple_subscribers() -> Result<()> {
    let harness = ServerHarness::start().await?;

    let mut sub_a = SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let mut stream_a = sub_a
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default())
        .await?
        .into_inner();

    let mut sub_b = SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let mut stream_b = sub_b
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default())
        .await?
        .into_inner();

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let resp = forking
        .advance_checkpoint(AdvanceCheckpointRequest {})
        .await?
        .into_inner();
    let expected_seq = resp.checkpoint_sequence_number;

    let msg_a = tokio::time::timeout(STREAM_RECV_TIMEOUT, stream_a.message())
        .await?
        .map_err(|e| anyhow!("stream A error: {e}"))?
        .ok_or_else(|| anyhow!("stream A closed before advance"))?;
    let msg_b = tokio::time::timeout(STREAM_RECV_TIMEOUT, stream_b.message())
        .await?
        .map_err(|e| anyhow!("stream B error: {e}"))?
        .ok_or_else(|| anyhow!("stream B closed before advance"))?;

    assert_eq!(msg_a.cursor, Some(expected_seq));
    assert_eq!(msg_b.cursor, Some(expected_seq));

    Ok(())
}

#[tokio::test]
async fn advance_clock_creates_and_streams_checkpoint() -> Result<()> {
    let harness = ServerHarness::start().await?;

    let mut subscriptions =
        SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let mut stream = subscriptions
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default())
        .await?
        .into_inner();

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let clock = forking
        .advance_clock(AdvanceClockRequest {
            duration_ms: Some(10),
        })
        .await?
        .into_inner();

    let msg = tokio::time::timeout(STREAM_RECV_TIMEOUT, stream.message())
        .await?
        .map_err(|e| anyhow!("stream error: {e}"))?
        .ok_or_else(|| anyhow!("subscription stream closed before clock advance"))?;
    let checkpoint_sequence_number = msg
        .cursor
        .ok_or_else(|| anyhow!("missing cursor on subscription message"))?;
    let status = forking.get_status(GetStatusRequest {}).await?.into_inner();

    assert_eq!(
        status.checkpoint_sequence_number,
        checkpoint_sequence_number
    );
    assert_eq!(status.timestamp_ms, clock.timestamp_ms);

    Ok(())
}
