// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    gas_coin::GAS,
    transaction::{CallArg, ObjectArg},
};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_object_balance_withdraw_disabled() {
    telemetry_subscribers::init_for_testing();

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.set_enable_accumulators_for_testing(false);
        cfg
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let tx = test_cluster
        .test_transaction_builder()
        .await
        .publish_examples("object_balance")
        .build();
    let resp = test_cluster.sign_and_execute_transaction(&tx).await;
    let package_id = resp.get_new_package_obj().unwrap().0;

    let tx = test_cluster
        .test_transaction_builder()
        .await
        .move_call(package_id, "object_balance", "new", vec![])
        .build();
    let resp = test_cluster.sign_and_execute_transaction(&tx).await;
    let vault_obj = resp
        .effects
        .unwrap()
        .created()
        .first()
        .unwrap()
        .reference
        .to_object_ref();

    let tx = test_cluster
        .test_transaction_builder()
        .await
        .move_call(
            package_id,
            "object_balance",
            "withdraw_funds",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(vault_obj)),
                CallArg::from(100u64),
            ],
        )
        .with_type_args(vec![GAS::type_tag()])
        .build();
    let tx = test_cluster.sign_transaction(&tx).await;
    let resp = test_cluster
        .wallet
        .execute_transaction_may_fail(tx)
        .await
        .unwrap();
    let effects = resp.effects.unwrap();
    let SuiExecutionStatus::Failure { error } = effects.status() else {
        panic!("Transaction should fail");
    };
    assert!(error.contains("MoveAbort"));
}
