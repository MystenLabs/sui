// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the checkpoint subscription gRPC. They spin up the full tonic stack
//! (forking admin RPCs plus the canonical sui-rpc-api streaming RPC), drive checkpoint-producing
//! admin calls, and assert subscribers see each checkpoint on the stream.

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
use sui_rpc_api::Client as RpcClient;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_api::proto::sui::rpc::v2::SenderFilter;
use sui_rpc_api::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc_api::proto::sui::rpc::v2::SubscribeEventsRequest;
use sui_rpc_api::proto::sui::rpc::v2::SubscribeTransactionsRequest;
use sui_rpc_api::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc_api::proto::sui::rpc::v2::TransactionLiteral;
use sui_rpc_api::proto::sui::rpc::v2::TransactionTerm;
use sui_rpc_api::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::transaction_literal::Predicate;
use sui_rpc_api::subscription::SubscriptionService;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::KeypairTraits;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::RpcStateReader;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;

use crate::AdvanceCheckpointRequest;
use crate::AdvanceClockRequest;
use crate::ForkingServiceClient;
use crate::GetStatusRequest;
use crate::context::Context;
use crate::proto::forking::forking_service_server::ForkingServiceServer;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::rpc::forking_service::ForkingServiceImpl;
use crate::services::ServiceManager;
use crate::store::ForkStore;

/// In-process gRPC harness. It builds a fresh Simulacrum from a genesis `NetworkConfig`, wires up
/// the subscription broker, and starts a tonic server on an ephemeral port. The server task is
/// aborted when the harness is dropped.
struct ServerHarness {
    server_task: tokio::task::JoinHandle<()>,
    grpc_endpoint: String,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object: Object,
    reference_gas_price: u64,
    // Held to keep the RPC store alive for the lifetime of the server.
    // Held to keep the metadata and RPC store directory alive for the server lifetime.
    _temp: tempfile::TempDir,
    // Held so remote object probes keep resolving to "not found"; tests that
    // exercise GetServiceInfo or GetTransaction mount additional mocks here.
    gql_server: wiremock::MockServer,
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
            store.clone(),
            rng,
        );
        let reference_gas_price = sim.reference_gas_price();
        let (sender, sender_key) = {
            let (addr, key) = sim
                .keystore()
                .accounts()
                .next()
                .expect("at least one account");
            (*addr, key.copy())
        };
        let gas_object = config
            .genesis
            .objects()
            .iter()
            .find(|obj| obj.owner == Owner::AddressOwner(sender) && obj.is_gas_coin())
            .expect("sender should have a gas coin in genesis")
            .clone();

        let registry = Registry::new();
        services.set_indexed_checkpoint_for_testing(forked_at_checkpoint)?;
        let initial_tip = services.initial_subscription_tip()?;
        let (checkpoint_sender, subscription_handle) = SubscriptionService::build(
            &registry,
            Some(services.indexed_checkpoint_fn()),
            None,
            None,
            None,
            Some(initial_tip),
        );

        // Service-backed on purpose: subscribers are published to by the
        // indexer's broadcast pipeline, so a service-less context would
        // exercise a publication path production never takes.
        let context = Arc::new(
            Context::new(sim, services, checkpoint_sender, &registry)
                .await
                .expect("service-backed context should initialize"),
        );

        let reader: Arc<dyn RpcStateReader> = Arc::new(store);
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
                    sender,
                    sender_key,
                    gas_object,
                    reference_gas_price,
                    _temp: temp,
                    gql_server,
                });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(anyhow!("timed out waiting for gRPC server to bind"))
    }

    fn build_transfer_tx(&self, gas_ref: ObjectRef, amount: u64) -> Transaction {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(SuiAddress::random_for_testing_only(), Some(amount));
            builder.finish()
        };
        let tx_data = TransactionData::new_with_gas_data(
            TransactionKind::ProgrammableTransaction(pt),
            self.sender,
            GasData {
                payment: vec![gas_ref],
                owner: self.sender,
                price: self.reference_gas_price,
                budget: 100_000_000,
            },
        );
        Transaction::from_data_and_signer(tx_data, vec![&self.sender_key])
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

fn sender_filter(address: SuiAddress) -> TransactionFilter {
    let mut sender = SenderFilter::default();
    sender.address = Some(address.to_string());
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(Predicate::Sender(sender));
    let mut term = TransactionTerm::default();
    term.literals = vec![literal];
    let mut filter = TransactionFilter::default();
    filter.terms = vec![term];
    filter
}

#[tokio::test]
async fn idle_fork_emits_initial_start_frames_without_new_checkpoints() -> Result<()> {
    let harness = ServerHarness::start().await?;
    let mut subscriptions =
        SubscriptionServiceClient::connect(harness.grpc_endpoint.clone()).await?;

    // Fresh idle fork at genesis (checkpoint 0): streams whose contract begins
    // with a progress-only start frame must yield it immediately, without
    // waiting for checkpoint 1.
    let mut tx_stream = subscriptions
        .subscribe_transactions(SubscribeTransactionsRequest::default())
        .await?
        .into_inner();
    let mut ev_stream = subscriptions
        .subscribe_events(SubscribeEventsRequest::default())
        .await?
        .into_inner();
    let mut filtered_cp_request = SubscribeCheckpointsRequest::default();
    filtered_cp_request.filter = Some(sender_filter(harness.sender));
    let mut filtered_cp = subscriptions
        .subscribe_checkpoints(filtered_cp_request)
        .await?
        .into_inner();

    let tx_frame = tokio::time::timeout(STREAM_RECV_TIMEOUT, tx_stream.message())
        .await?
        .map_err(|e| anyhow!("transaction stream error: {e}"))?
        .ok_or_else(|| anyhow!("transaction stream closed before start frame"))?;
    assert!(tx_frame.transaction.is_none());
    assert_eq!(
        tx_frame
            .watermark
            .as_ref()
            .and_then(|watermark| watermark.checkpoint),
        Some(0)
    );

    let ev_frame = tokio::time::timeout(STREAM_RECV_TIMEOUT, ev_stream.message())
        .await?
        .map_err(|e| anyhow!("event stream error: {e}"))?
        .ok_or_else(|| anyhow!("event stream closed before start frame"))?;
    assert!(ev_frame.event.is_none());
    assert_eq!(
        ev_frame
            .watermark
            .as_ref()
            .and_then(|watermark| watermark.checkpoint),
        Some(0)
    );

    let cp_frame = tokio::time::timeout(STREAM_RECV_TIMEOUT, filtered_cp.message())
        .await?
        .map_err(|e| anyhow!("checkpoint stream error: {e}"))?
        .ok_or_else(|| anyhow!("checkpoint stream closed before start frame"))?;
    assert!(cp_frame.checkpoint.is_none());
    assert_eq!(cp_frame.cursor, Some(0));

    // After checkpoint 1 is committed, a newly registered subscriber on the now-idle
    // fork must immediately receive its start frame at checkpoint 1 without waiting
    // for a checkpoint 2.
    let mut forking = ForkingServiceClient::connect(harness.grpc_endpoint.clone()).await?;
    let advanced = forking
        .advance_checkpoint(AdvanceCheckpointRequest {})
        .await?
        .into_inner();
    assert_eq!(advanced.checkpoint_sequence_number, 1);

    let mut late_tx = subscriptions
        .subscribe_transactions(SubscribeTransactionsRequest::default())
        .await?
        .into_inner();
    let late_frame = tokio::time::timeout(STREAM_RECV_TIMEOUT, late_tx.message())
        .await?
        .map_err(|e| anyhow!("late transaction stream error: {e}"))?
        .ok_or_else(|| anyhow!("late transaction stream closed before start frame"))?;
    assert!(late_frame.transaction.is_none());
    assert_eq!(
        late_frame
            .watermark
            .as_ref()
            .and_then(|watermark| watermark.checkpoint),
        Some(1)
    );

    Ok(())
}

#[tokio::test]
async fn idle_fork_execute_transaction_and_wait_for_checkpoint_succeeds() -> Result<()> {
    use wiremock::Mock;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_string_contains;
    use wiremock::matchers::method;

    let harness = ServerHarness::start().await?;
    Mock::given(method("POST"))
        .and(body_string_contains("availableRange"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "serviceConfig": {
                    "availableRange": {
                        "first": { "sequenceNumber": 0 }
                    }
                }
            }
        })))
        .mount(&harness.gql_server)
        .await;
    Mock::given(method("POST"))
        .and(body_string_contains("transaction(digest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "transaction": null }
        })))
        .mount(&harness.gql_server)
        .await;

    let client = RpcClient::new(&harness.grpc_endpoint)?;
    let mut gas_ref = harness.gas_object.compute_object_reference();
    for i in 1..=5 {
        let tx = harness.build_transfer_tx(gas_ref, 1_000);
        let expected_digest = *tx.digest();
        let executed = tokio::time::timeout(
            STREAM_RECV_TIMEOUT,
            client.execute_transaction_and_wait_for_checkpoint(&tx),
        )
        .await
        .map_err(|_| {
            anyhow!("execute_transaction_and_wait_for_checkpoint timed out on tx {i}")
        })??;
        assert_eq!(executed.transaction.digest(), expected_digest);
        gas_ref = executed.effects.gas_object().unwrap().0;
    }

    Ok(())
}
