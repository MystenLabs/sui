// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc_api::grpc::v2beta2::event_service_proto::event_service_client::EventServiceClient;
use sui_rpc_api::grpc::v2beta2::event_service_proto::{Event, ListAuthenticatedEventsRequest};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;

#[derive(Deserialize, Serialize)]
struct AuthEventPayload {
    value: u64,
}

#[sim_test]
async fn list_authenticated_events_end_to_end() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_accumulators_for_testing();
            cfg
        });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/rpc/data/auth_event");
    let (package_id, _, _) =
        { sui_test_transaction_builder::publish_package(&test_cluster.wallet, path).await };

    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    for i in 1..=10u64 {
        let emit_value = 100 + i;
        let mut ptb_i = ProgrammableTransactionBuilder::new();
        let val_i = ptb_i.pure(emit_value).unwrap();
        ptb_i.programmable_move_call(
            package_id,
            move_core_types::identifier::Identifier::new("events").unwrap(),
            move_core_types::identifier::Identifier::new("emit").unwrap(),
            vec![],
            vec![val_i],
        );
        let tx_data_i = TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb_i.finish()),
            sender,
            {
                let wallet = &mut test_cluster.wallet;
                wallet
                    .gas_objects(sender)
                    .await
                    .unwrap()
                    .pop()
                    .unwrap()
                    .1
                    .object_ref()
            },
            10_000_000,
            rgp,
        );
        test_cluster.sign_and_execute_transaction(&tx_data_i).await;
    }

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let req = ListAuthenticatedEventsRequest {
        stream_id: Some(package_id.to_string()),
        start_checkpoint: Some(0),
        limit: Some(1000),
    };
    let resp = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();
    let count = resp.events.len();
    assert_eq!(count, 10, "expected 10 authenticated events, got {count}");
    let found = resp.events.iter().any(|e| match &e.event {
        Some(Event {
            contents: Some(bcs),
            ..
        }) => !bcs.value.clone().unwrap_or_default().is_empty(),
        _ => false,
    });
    assert!(found, "expected authenticated event for the stream");
}

#[sim_test]
async fn list_authenticated_events_limit_validation() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let req = ListAuthenticatedEventsRequest {
        stream_id: Some(sender.to_string()),
        start_checkpoint: Some(0),
        limit: Some(1001),
    };
    let err = client
        .list_authenticated_events(req)
        .await
        .expect_err("expected InvalidArgument for limit > 1000");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[sim_test]
async fn list_authenticated_events_start_beyond_highest() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let probe = ListAuthenticatedEventsRequest {
        stream_id: Some(sender.to_string()),
        start_checkpoint: Some(0),
        limit: Some(1),
    };
    let highest = client
        .list_authenticated_events(probe)
        .await
        .unwrap()
        .into_inner()
        .last_checkpoint
        .unwrap_or(0);

    let req = ListAuthenticatedEventsRequest {
        stream_id: Some(sender.to_string()),
        start_checkpoint: Some(highest.saturating_add(100)),
        limit: Some(10),
    };
    let resp = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();
    assert!(resp.events.is_empty());
    assert_eq!(resp.last_checkpoint, Some(highest));
}

#[sim_test]
async fn list_authenticated_events_empty_gap_multiple_checkpoints() {
    let mut test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Get current highest checkpoint
    let probe = ListAuthenticatedEventsRequest {
        stream_id: Some(sender.to_string()),
        start_checkpoint: Some(0),
        limit: Some(1),
    };
    let highest_before = client
        .list_authenticated_events(probe)
        .await
        .unwrap()
        .into_inner()
        .last_checkpoint
        .unwrap_or(0);

    // Submit transactions without authenticated events
    let rgp = test_cluster.get_reference_gas_price().await;
    for _ in 0..3u64 {
        let ptb =
            sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder::new();
        let tx_data = sui_types::transaction::TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb.finish()),
            sender,
            {
                let wallet = &mut test_cluster.wallet;
                wallet
                    .gas_objects(sender)
                    .await
                    .unwrap()
                    .pop()
                    .unwrap()
                    .1
                    .object_ref()
            },
            10_000_000,
            rgp,
        );
        test_cluster.sign_and_execute_transaction(&tx_data).await;
    }

    let req = ListAuthenticatedEventsRequest {
        stream_id: Some(sender.to_string()),
        start_checkpoint: Some(highest_before.saturating_add(1)),
        limit: Some(3),
    };
    let resp = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();
    assert!(resp.events.is_empty());
    assert_eq!(resp.last_checkpoint, Some(highest_before.saturating_add(3)));
    assert!(resp.proof.is_none());
}
