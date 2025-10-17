// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse};
use sui_keys::keystore::AccountKeystore;
use sui_light_client::mmr::apply_stream_updates;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::accumulator_root::EventCommitment;
use sui_types::{
    accumulator_root::{self as ar, EventStreamHead},
    base_types::{ObjectID, SuiAddress},
    dynamic_field::Field,
    effects::{AccumulatorValue, TransactionEffectsAPI},
    messages_checkpoint::CheckpointSequenceNumber,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::TransactionData,
};
use test_cluster::{TestCluster, TestClusterBuilder};

async fn setup_test_cluster_with_auth_events() -> TestCluster {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    TestClusterBuilder::new().build().await
}

async fn publish_auth_event_package(test_cluster: &TestCluster) -> ObjectID {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data/auth_event");
    let (package_id, _, _) =
        sui_test_transaction_builder::publish_package(&test_cluster.wallet, path).await;
    package_id
}

async fn try_emit_authenticated_event(
    test_cluster: &mut TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    value: u64,
) -> anyhow::Result<SuiTransactionBlockResponse> {
    let rgp = test_cluster.get_reference_gas_price().await;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let val = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        Identifier::new("events").unwrap(),
        Identifier::new("emit").unwrap(),
        vec![],
        vec![val],
    );
    let tx_data = TransactionData::new(
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

    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    test_cluster.wallet.execute_transaction_may_fail(tx).await
}

async fn load_event_stream_head_by_object_id(
    state: &sui_core::authority::AuthorityState,
    object_id: ObjectID,
) -> Option<EventStreamHead> {
    let obj = state.get_object(&object_id).await?;
    let mo = obj.data.try_as_move()?;
    let field = mo.to_rust::<Field<ar::AccumulatorKey, EventStreamHead>>()?;
    Some(field.value)
}

fn build_event_commitments_from_checkpoint(
    state: &sui_core::authority::AuthorityState,
    checkpoint_seq: CheckpointSequenceNumber,
) -> Result<Vec<EventCommitment>, Box<dyn std::error::Error>> {
    let checkpoint_contents = state.get_checkpoint_contents_by_sequence_number(checkpoint_seq)?;
    let mut event_commitments = Vec::new();

    for (idx, digest) in checkpoint_contents.iter().enumerate() {
        let effects = state
            .get_transaction_cache_reader()
            .get_executed_effects(&digest.transaction)
            .ok_or_else(|| format!("Missing effects for transaction {}", digest.transaction))?;

        let accumulator_events = effects.accumulator_events();
        for acc_event in accumulator_events.iter() {
            if let AccumulatorValue::EventDigest(event_idx, digest) = &acc_event.write.value {
                let event_commitment =
                    EventCommitment::new(checkpoint_seq, idx as u64, *event_idx, *digest);
                event_commitments.push(event_commitment);
            }
        }
    }

    Ok(event_commitments)
}

#[sim_test]
async fn authenticated_events_single_event_test() {
    let mut test_cluster = setup_test_cluster_with_auth_events().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let resp = try_emit_authenticated_event(&mut test_cluster, package_id, sender, 42)
        .await
        .expect("Transaction should succeed");

    let effects = resp.effects.as_ref().unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction effects should be successful"
    );

    let acc_events = effects.accumulator_events();
    assert_eq!(acc_events.len(), 1, "Expected 1 accumulator event");

    let event = &acc_events[0];
    assert_eq!(
        event.address,
        SuiAddress::from(AccountAddress::from(package_id))
    );

    let state = test_cluster.fullnode_handle.sui_node.state();
    let stream_head = load_event_stream_head_by_object_id(&state, event.accumulator_obj)
        .await
        .expect("EventStreamHead should be available");

    assert_eq!(stream_head.num_events, 1);
    assert_eq!(stream_head.mmr.len(), 1);
}

#[sim_test]
async fn authenticated_events_multiple_events_test() {
    let mut test_cluster = setup_test_cluster_with_auth_events().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let mut last_event_obj_id = None;

    for i in 0..10 {
        let resp = try_emit_authenticated_event(&mut test_cluster, package_id, sender, 100 + i)
            .await
            .expect("Transaction should succeed");

        let effects = resp.effects.as_ref().unwrap();
        assert!(
            effects.status().is_ok(),
            "Transaction effects should be successful"
        );

        let acc_events = effects.accumulator_events();
        assert_eq!(acc_events.len(), 1);
        assert_eq!(
            acc_events[0].address,
            SuiAddress::from(AccountAddress::from(package_id))
        );
        last_event_obj_id = Some(acc_events[0].accumulator_obj);
    }

    tracing::info!("package_id: {package_id:?}, last_event_obj_id: {last_event_obj_id:?}");

    let state = test_cluster.fullnode_handle.sui_node.state();
    let stream_head = load_event_stream_head_by_object_id(&state, last_event_obj_id.unwrap())
        .await
        .expect("EventStreamHead should be available");

    assert_eq!(stream_head.num_events, 10);
    assert!(stream_head.mmr.len() > 1);
}

#[sim_test]
async fn authenticated_events_disabled_test() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let result = try_emit_authenticated_event(&mut test_cluster, package_id, sender, 42).await;

    let response = result.expect("Transaction should execute to effects");
    let effects = response.effects.as_ref().unwrap();

    assert!(
        effects.status().is_err(),
        "Transaction should have failed when authenticated events are disabled"
    );

    let acc_events = effects.accumulator_events();
    assert_eq!(
        acc_events.len(),
        0,
        "No accumulator events should be generated when feature is disabled"
    );

    let error_str = format!("{:?}", effects.status());
    assert!(
        error_str.contains("0"),
        "Error should contain abort code 0 (NOT_SUPPORTED): {}",
        error_str
    );
}

#[sim_test]
async fn authenticated_events_mmr_validation_test() {
    let mut test_cluster = setup_test_cluster_with_auth_events().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let state = test_cluster.fullnode_handle.sui_node.state();

    let start_checkpoint = state.get_latest_checkpoint_sequence_number().unwrap();

    let mut accumulator_obj_id = None;
    for i in 0..3 {
        let resp = try_emit_authenticated_event(&mut test_cluster, package_id, sender, 100 + i)
            .await
            .expect("Transaction should succeed");

        let effects = resp.effects.as_ref().unwrap();
        let acc_events = effects.accumulator_events();
        if !acc_events.is_empty() {
            accumulator_obj_id = Some(acc_events[0].accumulator_obj);
        }
    }

    let checkpoint_x = state.get_latest_checkpoint_sequence_number().unwrap();
    assert!(
        checkpoint_x > start_checkpoint,
        "Checkpoint should have advanced"
    );

    let accumulator_obj_id = accumulator_obj_id.expect("Should have accumulator object ID");
    let sui_stream_head_x = load_event_stream_head_by_object_id(&state, accumulator_obj_id)
        .await
        .expect("Should be able to load event stream head at checkpoint X");

    let mut events_up_to_x = Vec::new();
    for cp_seq in (start_checkpoint + 1)..=checkpoint_x {
        let cp_events = build_event_commitments_from_checkpoint(&state, cp_seq)
            .expect("Should be able to build event commitments");
        if !cp_events.is_empty() {
            events_up_to_x.push(cp_events);
        }
    }

    let calculated_stream_head_x = apply_stream_updates(&EventStreamHead::new(), events_up_to_x);

    assert_eq!(
        calculated_stream_head_x.num_events, sui_stream_head_x.num_events,
        "Event count should match at checkpoint X"
    );

    assert_eq!(
        calculated_stream_head_x.mmr, sui_stream_head_x.mmr,
        "MMR should match at checkpoint X"
    );

    let stream_head_x = &sui_stream_head_x;

    for i in 0..4 {
        let _resp = try_emit_authenticated_event(&mut test_cluster, package_id, sender, 200 + i)
            .await
            .expect("Transaction should succeed");
    }

    let checkpoint_x_prime = state.get_latest_checkpoint_sequence_number().unwrap();
    assert!(
        checkpoint_x_prime > checkpoint_x,
        "Checkpoint should have advanced further"
    );

    let mut events_between = Vec::new();
    for cp_seq in (checkpoint_x + 1)..=checkpoint_x_prime {
        let cp_events = build_event_commitments_from_checkpoint(&state, cp_seq)
            .expect("Should be able to build event commitments");
        if !cp_events.is_empty() {
            events_between.push(cp_events);
        }
    }

    let actual_stream_head_x_prime =
        load_event_stream_head_by_object_id(&state, accumulator_obj_id)
            .await
            .expect("Should be able to load event stream head at checkpoint X'");

    let calculated_stream_head_x_prime = apply_stream_updates(stream_head_x, events_between);

    assert_eq!(
        calculated_stream_head_x_prime.num_events, actual_stream_head_x_prime.num_events,
        "Event count should match between calculated and actual stream heads"
    );

    assert_eq!(
        calculated_stream_head_x_prime.mmr, actual_stream_head_x_prime.mmr,
        "MMR should match between calculated and actual stream heads"
    );
}
