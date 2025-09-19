// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, identifier::Identifier, u256::U256};
use serde::Deserialize;
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse};
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    accumulator_root as ar,
    base_types::{ObjectID, SuiAddress},
    dynamic_field::Field,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::TransactionData,
};
use test_cluster::{TestCluster, TestClusterBuilder};

#[derive(Deserialize)]
#[allow(dead_code)]
struct MoveEventStreamHead {
    mmr: Vec<U256>,
    checkpoint_seq: u64,
    num_events: u64,
}

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

async fn emit_authenticated_event(
    test_cluster: &mut TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    value: u64,
) -> SuiTransactionBlockResponse {
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
    test_cluster.sign_and_execute_transaction(&tx_data).await
}

async fn load_event_stream_head_by_object_id(
    state: &sui_core::authority::AuthorityState,
    object_id: ObjectID,
) -> Option<MoveEventStreamHead> {
    let obj = state.get_object(&object_id).await?;
    let mo = obj.data.try_as_move()?;
    let field = mo.to_rust::<Field<ar::AccumulatorKey, MoveEventStreamHead>>()?;
    Some(field.value)
}

#[sim_test]
async fn authenticated_events_single_event_test() {
    let mut test_cluster = setup_test_cluster_with_auth_events().await;
    let package_id = publish_auth_event_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let resp = emit_authenticated_event(&mut test_cluster, package_id, sender, 42).await;

    let acc_events = resp.effects.as_ref().unwrap().accumulator_events();
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
        let resp = emit_authenticated_event(&mut test_cluster, package_id, sender, 100 + i).await;
        let acc_events = resp.effects.as_ref().unwrap().accumulator_events();
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
