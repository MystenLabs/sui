// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for `ForkNode`. They serve a synthetic fork over the full tonic stack (forking
//! admin RPCs plus the canonical sui-rpc-api streaming RPC), drive it in-process and over gRPC,
//! and assert on what subscribers and later opens of the data directory observe.

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
use sui_rpc_api::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc_api::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_rpc_api::subscription::SubscriptionService;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::digests::ChainIdentifier;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::ForkNode;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::context::Context;
use crate::services::ServiceManager;
use crate::startup;
use crate::startup::ForkParts;
use crate::store::ForkStore;

/// In-process harness that serves a synthetic fork on an ephemeral port.
struct ServerHarness {
    node: ForkNode,
    grpc_endpoint: String,
    forked_at_checkpoint: CheckpointSequenceNumber,
    chain_identifier: ChainIdentifier,
    // Held to keep the metadata and RPC store directory alive for the server lifetime.
    temp: tempfile::TempDir,
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
        let chain_identifier: ChainIdentifier = (*genesis_checkpoint.digest()).into();
        let mut services = ServiceManager::open(
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
        store.save_checkpoint(&genesis_checkpoint, &genesis_contents)?;
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
            chain_identifier,
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
        let simulacrum = Arc::new(tokio::sync::RwLock::new(sim));
        let indexer_service = services
            .start_indexer(simulacrum.clone(), checkpoint_sender, &registry)
            .await
            .expect("indexer service should start");
        let context = Context::new(simulacrum, services);
        let listener = startup::bind("127.0.0.1:0".parse()?).await?;
        let node = ForkNode::from_parts(
            ForkParts {
                context,
                subscription_handle,
                indexer_service,
                data_dir: temp.path().to_path_buf(),
                resumed: false,
            },
            listener,
            "test",
        )
        .await?;
        let grpc_endpoint = format!("http://{}", node.rpc_address());

        Ok(Self {
            node,
            grpc_endpoint,
            forked_at_checkpoint,
            chain_identifier,
            temp,
            _gql_server: gql_server,
        })
    }
}

const STREAM_RECV_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn stop_stops_rpc_server() -> Result<()> {
    let harness = ServerHarness::start().await?;
    let grpc_endpoint = harness.grpc_endpoint.clone();

    assert_ne!(harness.node.rpc_address().port(), 0);
    ForkingServiceClient::connect(grpc_endpoint.clone()).await?;

    harness.node.stop().await?;

    assert!(ForkingServiceClient::connect(grpc_endpoint).await.is_err());
    Ok(())
}

#[tokio::test]
async fn into_service_shutdown_stops_rpc_server() -> Result<()> {
    let harness = ServerHarness::start().await?;
    let grpc_endpoint = harness.grpc_endpoint.clone();
    let service = harness.node.into_service();
    ForkingServiceClient::connect(grpc_endpoint.clone()).await?;

    service.shutdown().await?;

    assert!(ForkingServiceClient::connect(grpc_endpoint).await.is_err());
    Ok(())
}

#[tokio::test]
async fn in_process_administration_matches_grpc_status() -> Result<()> {
    let harness = ServerHarness::start().await?;
    let before = harness.node.status().await;

    let clock = harness
        .node
        .advance_clock(Duration::from_millis(1_000))
        .await;
    let sealed = harness.node.advance_checkpoint().await;

    assert_eq!(clock.timestamp_ms, before.timestamp_ms + 1_000);
    assert_eq!(
        sealed.checkpoint_sequence_number,
        before.checkpoint_sequence_number + 2
    );
    assert_eq!(sealed.timestamp_ms, clock.timestamp_ms);

    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let status = forking.get_status(GetStatusRequest {}).await?.into_inner();
    assert_eq!(status, harness.node.status().await);
    assert_eq!(
        status.checkpoint_sequence_number,
        sealed.checkpoint_sequence_number
    );
    assert_eq!(status.timestamp_ms, clock.timestamp_ms);
    Ok(())
}

/// A stopped node must release its rpc-store, because resuming the same directory reopens it.
#[tokio::test]
async fn stop_releases_the_data_directory_for_resume() -> Result<()> {
    let harness = ServerHarness::start().await?;
    let sealed = harness.node.advance_checkpoint().await;
    let ServerHarness {
        node,
        forked_at_checkpoint,
        chain_identifier,
        temp,
        _gql_server,
        ..
    } = harness;

    node.stop().await?;

    let services = ServiceManager::open(
        temp.path(),
        "localnet".to_owned(),
        forked_at_checkpoint,
        chain_identifier,
    )?;
    let store = ForkStore::new_for_testing(temp.path().to_path_buf(), services.local_store());
    let base = startup::resume_base_checkpoint(&store)?;
    assert_eq!(
        base.data().sequence_number,
        sealed.checkpoint_sequence_number
    );
    Ok(())
}

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
