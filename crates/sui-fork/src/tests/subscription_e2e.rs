// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the checkpoint subscription gRPC. They spin up the full tonic stack
//! (forking admin RPCs plus the canonical sui-rpc-api streaming RPC), drive checkpoint-producing
//! admin calls, and assert subscribers see each checkpoint on the stream.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use prometheus::Registry;
use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_rpc_api::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc_api::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_rpc_api::subscription::SubscriptionService;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::ForkNode;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::context::Context;
use crate::services::ServiceManager;
use crate::startup::ForkParts;
use crate::store::ForkStore;

/// In-process gRPC harness. It builds a fresh Simulacrum from a genesis `NetworkConfig`, assembles
/// [`ForkParts`] over it, and serves through [`ForkNode::from_parts`] on an ephemeral port, which
/// is the same serving path production takes. Dropping the harness drops the `ForkNode`, which
/// aborts the fork's tasks.
struct ServerHarness {
    fork: ForkNode,
    grpc_endpoint: String,
    // Held to keep the metadata and RPC store directory alive for the server lifetime.
    _temp: tempfile::TempDir,
    // Held so remote object probes keep resolving to "not found".
    _gql_server: wiremock::MockServer,
}

impl ServerHarness {
    async fn start() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        let genesis_checkpoint = config.genesis.checkpoint();
        let genesis_contents = config.genesis.checkpoint_contents().clone();
        let forked_at_checkpoint = genesis_checkpoint.data().sequence_number;
        let chain_identifier = (*genesis_checkpoint.digest()).into();
        let services = ServiceManager::open(
            temp.path(),
            "localnet".to_owned(),
            forked_at_checkpoint,
            chain_identifier,
        )?;
        let gql_server = crate::test_support::absent_objects_gql_server().await;
        let mut store = ForkStore::new_for_testing_with_remote(
            temp.path().to_path_buf(),
            gql_server.uri(),
            forked_at_checkpoint,
            services.local_store(),
        );
        store
            .local_store()
            .save_checkpoint(&genesis_checkpoint, &genesis_contents)?;
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        store.update_objects(written, vec![]);

        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            genesis_checkpoint,
            config.genesis.sui_system_object(),
            &config,
            store,
            rng,
        );

        let registry = Registry::new();
        let (checkpoint_sender, subscription_handle) =
            SubscriptionService::build(&registry, None, None, None, None);

        // Service-backed on purpose: subscribers are published to by the
        // indexer's broadcast pipeline, so a service-less context would
        // exercise a publication path production never takes.
        let (context, indexer_service) = Context::new(sim, services, checkpoint_sender, &registry)
            .await
            .expect("service-backed context should initialize");

        let fork = ForkNode::from_parts(
            ForkParts {
                context,
                subscription_handle,
                indexer_service,
                data_dir: temp.path().to_path_buf(),
                network_name: "localnet".to_owned(),
                forked_at_checkpoint,
                starting_checkpoint: forked_at_checkpoint,
            },
            false,
            "127.0.0.1:0".parse()?,
            "test",
            &registry,
        )
        .await?;

        let grpc_endpoint = format!("http://{}", fork.rpc_address());
        Ok(Self {
            fork,
            grpc_endpoint,
            _temp: temp,
            _gql_server: gql_server,
        })
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

#[tokio::test]
async fn shutdown_stops_serving() -> Result<()> {
    let harness = ServerHarness::start().await?;
    assert_ne!(
        harness.fork.rpc_address().port(),
        0,
        "a requested port 0 should resolve to a real ephemeral port",
    );

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    forking.get_status(GetStatusRequest {}).await?;
    drop(forking);

    let ServerHarness {
        fork,
        grpc_endpoint,
        _temp,
        _gql_server,
    } = harness;
    fork.shutdown().await?;

    assert!(
        ForkingServiceClient::connect(grpc_endpoint).await.is_err(),
        "endpoint should stop accepting connections after shutdown",
    );

    Ok(())
}

#[tokio::test]
async fn in_process_admin_ops_share_the_grpc_contract() -> Result<()> {
    let harness = ServerHarness::start().await?;

    let advanced = harness
        .fork
        .advance_clock(Duration::from_millis(1_000))
        .await?;
    let created = harness.fork.create_checkpoint().await?;
    assert_eq!(
        created.sequence_number,
        advanced.checkpoint.sequence_number + 1
    );

    let status = harness.fork.status().await?;
    assert_eq!(status.checkpoint_sequence_number, created.sequence_number);
    assert_eq!(status.timestamp_ms, advanced.timestamp_ms);

    // The gRPC surface reports the same state the in-process handle does.
    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let grpc_status = forking.get_status(GetStatusRequest {}).await?.into_inner();
    assert_eq!(
        grpc_status.checkpoint_sequence_number,
        status.checkpoint_sequence_number
    );
    assert_eq!(grpc_status.timestamp_ms, status.timestamp_ms);
    assert_eq!(
        grpc_status.forked_at_checkpoint,
        status.forked_at_checkpoint
    );

    Ok(())
}
