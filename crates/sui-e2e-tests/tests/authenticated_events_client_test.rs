// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use move_core_types::identifier::Identifier;
use std::sync::Arc;
use sui_keys::keystore::AccountKeystore;
use sui_light_client::authenticated_events::AuthenticatedEventsClient;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc_api::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::ValidatorCommittee;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::Committee;
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

async fn setup_test_cluster() -> TestCluster {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await
}

async fn publish_auth_event_package(test_cluster: &TestCluster) -> ObjectID {
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
                .publish_async(path)
                .await
                .build(),
        )
        .await;

    let resp = test_cluster
        .wallet
        .execute_transaction_must_succeed(txn)
        .await;

    resp.get_new_package_obj().unwrap().0
}

async fn emit_events(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    count: u64,
) {
    let rgp = test_cluster.get_reference_gas_price().await;

    for i in 0..count {
        let mut ptb = ProgrammableTransactionBuilder::new();
        let val = ptb.pure(100 + i).unwrap();
        ptb.programmable_move_call(
            package_id,
            Identifier::new("events").unwrap(),
            Identifier::new("emit").unwrap(),
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
}

async fn emit_events_batch(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    start_value: u64,
    count: u64,
) {
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut ptb = ProgrammableTransactionBuilder::new();
    let start_val = ptb.pure(start_value).unwrap();
    let count_val = ptb.pure(count).unwrap();
    ptb.programmable_move_call(
        package_id,
        Identifier::new("events").unwrap(),
        Identifier::new("emit_multiple").unwrap(),
        vec![],
        vec![start_val, count_val],
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

async fn get_genesis_committee(test_cluster: &TestCluster) -> Committee {
    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let response = ledger_client
        .get_epoch(GetEpochRequest::new(0).with_read_mask(FieldMask::from_paths(["committee"])))
        .await
        .unwrap()
        .into_inner();

    let proto_committee = response.epoch.unwrap().committee.unwrap();

    let sdk_committee = ValidatorCommittee::try_from(&proto_committee).unwrap();
    Committee::from(sdk_committee)
}

#[sim_test]
async fn test_client_end_to_end_stream() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    emit_events(&test_cluster, package_id, sender, 5).await;

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 10).await;

    let mut received_count = 0;

    while received_count < 10 {
        if let Some(Ok(_verified_event)) = stream.next().await {
            received_count += 1;
        }
    }

    assert_eq!(received_count, 10);
}

#[sim_test]
async fn test_client_cross_epoch_stream() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .with_epoch_duration_ms(1000)
        .build()
        .await;

    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    emit_events(&test_cluster, package_id, sender, 3).await;

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 5).await;

    test_cluster.wait_for_epoch(None).await;

    emit_events(&test_cluster, package_id, sender, 5).await;

    let mut received_count = 0;
    let mut later_epoch_events = 0;

    while received_count < 10 {
        if let Some(Ok(verified_event)) = stream.next().await {
            let checkpoint_summary = test_cluster
                .fullnode_handle
                .sui_node
                .state()
                .get_checkpoint_summary_by_sequence_number(verified_event.checkpoint)
                .unwrap();
            let epoch = checkpoint_summary.epoch;

            if epoch > 0 {
                later_epoch_events += 1;
            }

            received_count += 1;
        }
    }

    assert_eq!(received_count, 10);
    assert!(
        later_epoch_events > 0,
        "No events recieved for epoch > 1 ~ no trust ratcheting performed"
    );
}

#[sim_test]
async fn test_client_resume_after_downtime() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    emit_events(&test_cluster, package_id, sender, 3).await;

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 5).await;

    let mut received_count = 0;
    let mut last_event = None;

    while received_count < 5 {
        if let Some(Ok(event)) = stream.next().await {
            last_event = Some(event);
            received_count += 1;
        }
    }

    let last_event = last_event.unwrap();
    let last_checkpoint = last_event.checkpoint;

    drop(stream);

    emit_events(&test_cluster, package_id, sender, 5).await;

    let mut resumed_stream = Box::pin(
        client
            .clone()
            .stream_events_from_checkpoint(stream_id, last_checkpoint)
            .await
            .unwrap(),
    );

    let mut resumed_count = 0;

    while resumed_count < 5 {
        if let Some(Ok(_verified_event)) = resumed_stream.next().await {
            resumed_count += 1;
        }
    }

    assert_eq!(resumed_count, 5);
}

#[sim_test]
async fn test_client_mmr_verification() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    emit_events(&test_cluster, package_id, sender, 5).await;

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 20).await;

    let mut received_count = 0;

    while received_count < 20 {
        if let Some(Ok(_verified_event)) = stream.next().await {
            received_count += 1;
        }
    }

    assert_eq!(received_count, 20);
}

#[sim_test]
async fn test_client_multiple_streams() {
    let test_cluster = setup_test_cluster().await;
    let package_id_1 = publish_auth_event_package(&test_cluster).await;
    let package_id_2 = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id_1 = SuiAddress::from(package_id_1);
    let stream_id_2 = SuiAddress::from(package_id_2);

    emit_events(&test_cluster, package_id_1, sender, 3).await;
    emit_events(&test_cluster, package_id_2, sender, 3).await;

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream1 = Box::pin(client.clone().stream_events(stream_id_1).await.unwrap());

    let mut stream2 = Box::pin(client.clone().stream_events(stream_id_2).await.unwrap());

    emit_events(&test_cluster, package_id_1, sender, 5).await;
    emit_events(&test_cluster, package_id_2, sender, 5).await;

    let handle1 = tokio::spawn(async move {
        let mut count = 0;
        while count < 5 {
            if let Some(Ok(_event)) = stream1.next().await {
                count += 1;
            }
        }
        count
    });

    let handle2 = tokio::spawn(async move {
        let mut count = 0;
        while count < 5 {
            if let Some(Ok(_event)) = stream2.next().await {
                count += 1;
            }
        }
        count
    });

    let (count1, count2) = tokio::join!(handle1, handle2);
    assert_eq!(count1.unwrap(), 5);
    assert_eq!(count2.unwrap(), 5);
}

#[sim_test]
async fn test_client_resume_from_checkpoint_without_events() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 5).await;

    let mut received_count = 0;

    while received_count < 5 {
        if let Some(Ok(_event)) = stream.next().await {
            received_count += 1;
        }
    }

    drop(stream);

    let checkpoint_with_no_events = 1;

    let result = client
        .clone()
        .stream_events_from_checkpoint(stream_id, checkpoint_with_no_events)
        .await;

    let Err(e) = result else {
        panic!("Should have failed to create stream from checkpoint without events");
    };

    let sui_light_client::authenticated_events::ClientError::InternalError(msg) = e else {
        panic!("Expected InternalError, got: {:?}", e);
    };

    assert!(
        msg.contains("Cannot resume from checkpoint")
            && msg.contains("EventStreamHead was not updated at this checkpoint"),
        "Expected error message to explain that EventStreamHead was not updated, got: {}",
        msg
    );
}

#[sim_test]
async fn test_client_pruned_checkpoint_error() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .with_epoch_duration_ms(5000)
        .build()
        .await;

    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    emit_events(&test_cluster, package_id, sender, 5).await;

    let first_checkpoint = 1;

    for _ in 0..2 {
        test_cluster.wait_for_epoch(None).await;
    }

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let result = client
        .clone()
        .stream_events_from_checkpoint(stream_id, first_checkpoint)
        .await;

    let Err(e) = result else {
        panic!("Expected error for pruned checkpoint, but stream creation succeeded");
    };

    assert!(
        matches!(
            e,
            sui_light_client::authenticated_events::ClientError::RpcError(_)
        ),
        "Expected RpcError for pruned checkpoint, got: {:?}",
        e
    );
}

#[sim_test]
async fn test_client_large_gap_with_pagination() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 10).await;

    let mut received_count = 0;
    let mut last_checkpoint = 0;

    while received_count < 10 {
        if let Some(Ok(event)) = stream.next().await {
            last_checkpoint = event.checkpoint;
            received_count += 1;
        }
    }

    drop(stream);

    let batches = 11;
    let batch_size = 100;

    for i in 0..batches {
        emit_events_batch(
            &test_cluster,
            package_id,
            sender,
            100 + i * batch_size,
            batch_size,
        )
        .await;
    }

    let mut resumed_stream = Box::pin(
        client
            .clone()
            .stream_events_from_checkpoint(stream_id, last_checkpoint)
            .await
            .unwrap(),
    );

    let mut resumed_count = 0;

    while resumed_count < batches * batch_size {
        if let Some(Ok(_event)) = resumed_stream.next().await {
            resumed_count += 1;
        }
    }

    assert_eq!(resumed_count, batches * batch_size);
}

#[sim_test]
async fn test_client_multiple_events_single_transaction() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events_batch(&test_cluster, package_id, sender, 100, 2).await;

    let mut received_count = 0;

    while received_count < 2 {
        if let Some(Ok(_verified_event)) = stream.next().await {
            received_count += 1;
        }
    }

    assert_eq!(received_count, 2);
}

#[sim_test]
async fn test_client_stream_nonexistent_stream() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new(test_cluster.rpc_url(), genesis_committee)
            .await
            .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 10).await;

    let mut received_count = 0;

    while received_count < 10 {
        if let Some(Ok(_verified_event)) = stream.next().await {
            received_count += 1;
        }
    }

    assert_eq!(received_count, 10);
}

#[sim_test]
async fn test_client_pagination_limit_forward_progress() {
    let test_cluster = setup_test_cluster().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let stream_id = SuiAddress::from(package_id);

    let config = sui_light_client::authenticated_events::ClientConfig::new(
        5,                                     /* page_size */
        std::time::Duration::from_millis(100), /* poll_interval */
        2,                                     /* max_pagination_iterations */
        std::time::Duration::from_secs(30),    /* rpc_timeout */
    )
    .unwrap();

    let genesis_committee = get_genesis_committee(&test_cluster).await;
    let client = Arc::new(
        AuthenticatedEventsClient::new_with_config(
            test_cluster.rpc_url(),
            genesis_committee,
            config,
        )
        .await
        .unwrap(),
    );

    let mut stream = Box::pin(client.clone().stream_events(stream_id).await.unwrap());

    emit_events(&test_cluster, package_id, sender, 3).await;

    let mut last_checkpoint = 0;
    let mut received_count = 0;

    while received_count < 3 {
        if let Some(Ok(event)) = stream.next().await {
            last_checkpoint = event.checkpoint;
            received_count += 1;
        }
    }

    drop(stream);

    // Emitting 24 events across 3 batches. With page_size=5 and max_pagination_iterations=2, this triggers pagination limit

    emit_events_batch(&test_cluster, package_id, sender, 100, 8).await;
    emit_events_batch(&test_cluster, package_id, sender, 200, 8).await;
    emit_events_batch(&test_cluster, package_id, sender, 300, 8).await;

    let mut resumed_stream = Box::pin(
        client
            .clone()
            .stream_events_from_checkpoint(stream_id, last_checkpoint)
            .await
            .unwrap(),
    );

    let total_new_events = 24;
    let mut new_received_count = 0;

    while new_received_count < total_new_events {
        if let Some(Ok(_verified_event)) = resumed_stream.next().await {
            new_received_count += 1;
        }
    }

    assert_eq!(
        new_received_count, total_new_events,
        "Should receive all events despite pagination limits"
    );
}
