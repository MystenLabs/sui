// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::Event;
use sui_rpc_api::grpc::alpha::event_service_proto::event_service_client::EventServiceClient;
use sui_rpc_api::grpc::alpha::event_service_proto::ListAuthenticatedEventsRequest;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::{TestCluster, TestClusterBuilder};

fn create_rpc_config_with_authenticated_events() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
        authenticated_events_indexing: Some(true),
        enable_indexing: Some(true),
        ..Default::default()
    }
}

async fn publish_test_package(test_cluster: &TestCluster) -> ObjectID {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data/auth_event");

    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let txn = test_cluster
        .wallet
        .sign_transaction(
            &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, 1000)
                .with_gas_budget(50_000_000_000)
                .publish(path)
                .build(),
        )
        .await;
    let resp = test_cluster
        .wallet
        .execute_transaction_must_succeed(txn)
        .await;
    resp.get_new_package_obj().unwrap().0
}

async fn emit_test_event(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    value: u64,
) {
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut ptb = ProgrammableTransactionBuilder::new();
    let val = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        move_core_types::identifier::Identifier::new("events").unwrap(),
        move_core_types::identifier::Identifier::new("emit").unwrap(),
        vec![],
        vec![val],
    );
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new(
        sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_object,
        50_000_000_000,
        rgp,
    );
    test_cluster.sign_and_execute_transaction(&tx_data).await;
}

async fn query_authenticated_events(
    rpc_url: &str,
    stream_id: &str,
    start_checkpoint: u64,
    end_checkpoint: Option<u64>,
    page_size: Option<u32>,
) -> Result<
    sui_rpc_api::grpc::alpha::event_service_proto::ListAuthenticatedEventsResponse,
    tonic::Status,
> {
    let mut client = EventServiceClient::connect(rpc_url.to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(stream_id.to_string());
    req.start_checkpoint = Some(start_checkpoint);
    req.end_checkpoint = end_checkpoint;
    req.page_size = page_size;
    req.page_token = None;

    client
        .list_authenticated_events(req)
        .await
        .map(|r| r.into_inner())
}

#[sim_test]
async fn list_authenticated_events_end_to_end() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..10 {
        emit_test_event(&test_cluster, package_id, sender, 100 + i).await;
    }

    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        None,
        None,
    )
    .await
    .unwrap();

    let count = response.events.len();
    assert_eq!(count, 10, "expected 10 authenticated events, got {count}");

    let found = response.events.iter().any(|event| match &event.event {
        Some(Event {
            contents: Some(bcs),
            ..
        }) => bcs.value.as_ref().is_some_and(|v| !v.is_empty()),
        _ => false,
    });
    assert!(found, "expected authenticated event for the stream");
}

#[sim_test]
async fn list_authenticated_events_page_size_validation() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        0,
        None,
        Some(1500),
    )
    .await
    .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_start_beyond_highest() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let probe_response = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        0,
        None,
        Some(1),
    )
    .await
    .unwrap();
    let highest = probe_response.end_checkpoint.unwrap_or(0);

    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        highest + 1000,
        None,
        Some(10),
    )
    .await
    .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_pruned_checkpoint_error() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        0,
        None,
        Some(10),
    )
    .await
    .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn authenticated_events_disabled_test() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let result = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        0,
        None,
        Some(10),
    )
    .await;

    assert!(
        result.is_err(),
        "Expected error when authenticated events indexing is disabled"
    );

    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unimplemented);
    assert!(error
        .message()
        .contains("Authenticated events indexing is disabled"));
}

#[sim_test]
async fn authenticated_events_backfill_test() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config_without_indexing = sui_config::RpcConfig {
        authenticated_events_indexing: Some(false),
        enable_indexing: Some(false),
        ..Default::default()
    };

    let mut test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config_without_indexing)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..5 {
        emit_test_event(&test_cluster, package_id, sender, 200 + i).await;
    }

    let rpc_url_with_indexing = {
        let mut new_fullnode_config = test_cluster
            .fullnode_config_builder()
            .build(&mut rand::rngs::OsRng, test_cluster.swarm.config());

        if let Some(ref mut rpc_config) = new_fullnode_config.rpc {
            rpc_config.enable_indexing = Some(true);
            rpc_config.authenticated_events_indexing = Some(true);
        }

        let new_fullnode_handle = test_cluster
            .start_fullnode_from_config(new_fullnode_config)
            .await;

        new_fullnode_handle.rpc_url.clone()
    };

    let start = tokio::time::Instant::now();
    let response = loop {
        let response = query_authenticated_events(
            &rpc_url_with_indexing,
            &package_id.to_string(),
            0,
            None,
            None,
        )
        .await
        .unwrap();

        if response.events.len() == 5 {
            break response;
        }

        if start.elapsed() > tokio::time::Duration::from_secs(30) {
            panic!(
                "Timeout waiting for backfill to complete. Found {} events, expected 5",
                response.events.len()
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    };

    let count = response.events.len();
    assert_eq!(
        count, 5,
        "expected 5 authenticated events after backfill, got {count}"
    );
}

#[sim_test]
async fn list_authenticated_events_with_end_checkpoint() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..10 {
        emit_test_event(&test_cluster, package_id, sender, 100 + i).await;
    }

    let probe_response = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        None,
        None,
    )
    .await
    .unwrap();
    let highest = probe_response.end_checkpoint.unwrap_or(0);

    let mid_checkpoint = highest / 2;
    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        Some(mid_checkpoint),
        None,
    )
    .await
    .unwrap();

    for event in &response.events {
        let cp = event.checkpoint.unwrap();
        assert!(
            cp <= mid_checkpoint,
            "event checkpoint {cp} exceeds end_checkpoint {mid_checkpoint}"
        );
    }
}

#[sim_test]
async fn list_authenticated_events_end_checkpoint_validation() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..5 {
        emit_test_event(&test_cluster, package_id, sender, 100 + i).await;
    }

    let probe_response = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        None,
        None,
    )
    .await
    .unwrap();
    let highest = probe_response.end_checkpoint.unwrap_or(0);

    let result = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        Some(highest + 1000),
        None,
    )
    .await;

    assert!(
        result.is_err(),
        "Expected error when end_checkpoint exceeds highest_indexed"
    );
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error
        .message()
        .contains("exceeds highest_indexed_checkpoint"));
}

#[sim_test]
async fn list_authenticated_events_pagination_with_end_checkpoint() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..10 {
        emit_test_event(&test_cluster, package_id, sender, 100 + i).await;
    }

    let probe_response = query_authenticated_events(
        test_cluster.rpc_url(),
        &package_id.to_string(),
        0,
        None,
        None,
    )
    .await
    .unwrap();
    let highest = probe_response.end_checkpoint.unwrap_or(0);

    let mid_checkpoint = highest / 2;

    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut all_events = Vec::new();
    let mut next_page_token = None;

    loop {
        let mut req = ListAuthenticatedEventsRequest::default();
        req.stream_id = Some(package_id.to_string());
        req.start_checkpoint = Some(0);
        req.end_checkpoint = Some(mid_checkpoint);
        req.page_size = Some(2);
        req.page_token = next_page_token.clone();

        let response = client
            .list_authenticated_events(req)
            .await
            .unwrap()
            .into_inner();

        for event in &response.events {
            let cp = event.checkpoint.unwrap();
            assert!(
                cp <= mid_checkpoint,
                "event checkpoint {cp} exceeds end_checkpoint {mid_checkpoint}"
            );
        }

        all_events.extend(response.events);

        if response.next_page_token.is_none() {
            break;
        }
        next_page_token = response.next_page_token;
    }

    assert!(
        !all_events.is_empty(),
        "Expected to fetch events with pagination"
    );
}
