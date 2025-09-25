// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc_api::grpc::alpha::event_service_proto::event_service_client::EventServiceClient;
use sui_rpc_api::grpc::alpha::event_service_proto::{Event, ListAuthenticatedEventsRequest};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn list_authenticated_events_end_to_end() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/rpc/data/auth_event");

    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let gas_price = 1000;
    let txn = test_cluster
        .wallet
        .sign_transaction(
            &sui_test_transaction_builder::TestTransactionBuilder::new(
                sender, gas_object, gas_price,
            )
            .with_gas_budget(50_000_000_000)
            .publish(path)
            .build(),
        )
        .await;
    let resp = test_cluster
        .wallet
        .execute_transaction_must_succeed(txn)
        .await;
    let package_id = resp.get_new_package_obj().unwrap().0;

    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    for i in 0..10 {
        let emit_value = 100 + i;
        let mut ptb_i = ProgrammableTransactionBuilder::new();
        let val_i = ptb_i.pure(emit_value as u64).unwrap();
        ptb_i.programmable_move_call(
            package_id,
            move_core_types::identifier::Identifier::new("events").unwrap(),
            move_core_types::identifier::Identifier::new("emit").unwrap(),
            vec![],
            vec![val_i],
        );
        let gas_object = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let tx_data_i = TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb_i.finish()),
            sender,
            gas_object,
            50_000_000_000,
            rgp,
        );
        test_cluster.sign_and_execute_transaction(&tx_data_i).await;
    }

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(package_id.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = None;
    req.page_token = None;
    let response = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();

    let count = response.events.len();
    assert_eq!(count, 10, "expected 10 authenticated events, got {count}");

    let found = response.events.iter().any(|event| match &event.event {
        Some(Event {
            contents: Some(bcs),
            ..
        }) => !bcs.value.clone().unwrap_or_default().is_empty(),
        _ => false,
    });
    assert!(found, "expected authenticated event for the stream");
}

#[sim_test]
async fn list_authenticated_events_page_size_validation() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(sender.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = Some(1500);
    req.page_token = None;
    let response = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();
    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_start_beyond_highest() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut probe = ListAuthenticatedEventsRequest::default();
    probe.stream_id = Some(sender.to_string());
    probe.start_checkpoint = Some(0);
    probe.page_size = Some(1);
    probe.page_token = None;
    let highest = client
        .list_authenticated_events(probe)
        .await
        .unwrap()
        .into_inner()
        .last_checkpoint
        .unwrap_or(0);

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(sender.to_string());
    req.start_checkpoint = Some(highest + 1000);
    req.page_size = Some(10);
    req.page_token = None;
    let response = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_empty_gap_multiple_checkpoints() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let rgp = test_cluster.get_reference_gas_price().await;

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    for _i in 0..3 {
        let gas_object = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap();

        if gas_object.is_none() {
            break;
        }

        let tx_data = TransactionData::new_transfer_sui(
            sender,
            sender,
            None,
            gas_object.unwrap(),
            rgp,
            50_000_000_000,
        );
        test_cluster.sign_and_execute_transaction(&tx_data).await;
    }

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(sender.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = Some(100);
    req.page_token = None;
    let response = client.list_authenticated_events(req).await.unwrap();

    assert!(response.into_inner().events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_pruned_checkpoint_error() {
    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(sender.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = Some(10);
    req.page_token = None;

    let response = client.list_authenticated_events(req).await.unwrap();

    assert!(response.into_inner().events.is_empty());
}
