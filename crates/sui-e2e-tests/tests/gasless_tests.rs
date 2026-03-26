// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use move_core_types::language_storage::TypeTag;
use sui_core::transaction_driver::SubmitTransactionOptions;
use sui_macros::*;
use sui_test_transaction_builder::FundSource;
use sui_types::{
    base_types::SuiAddress, effects::TransactionEffectsAPI, gas::GasCostSummary,
    messages_grpc::SubmitTxRequest, transaction,
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

async fn setup_gasless_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_gasless_for_testing();
            cfg
        }))
        .build()
        .await
}

async fn setup_custom_coin(test_env: &mut TestEnv, funding: &[(u64, SuiAddress)]) -> TypeTag {
    let total: u64 = funding.iter().map(|(amount, _)| amount).sum();
    let (publisher, coin_type) = test_env.setup_custom_coin().await;
    let tx = test_env
        .tx_builder(publisher)
        .transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(total),
            funding.to_vec(),
            coin_type.clone(),
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true));
    coin_type
}

fn assert_zero_gas(gas_summary: &GasCostSummary) {
    assert_eq!(gas_summary.computation_cost, 0);
    assert_eq!(gas_summary.storage_cost, 0);
    assert_eq!(gas_summary.storage_rebate, 0);
    assert_eq!(gas_summary.non_refundable_storage_fee, 0);
}

// drive_transaction computes amplification_factor = gas_price / rgp, which would
// reject gasless transactions (gas_price=0) with GasPriceUnderRGP. This test
// verifies the bypass for that check works through the full orchestrator path.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_drive_transaction() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_type = setup_custom_coin(&mut test_env, &[(5000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx_data = test_env.create_gasless_transaction(1000, coin_type, sender, recipient, 0, 0);
    let signed_tx = test_env.cluster.wallet.sign_transaction(&tx_data).await;

    let orchestrator = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

    let result = orchestrator
        .transaction_driver()
        .drive_transaction(
            SubmitTxRequest::new_transaction(signed_tx),
            SubmitTransactionOptions {
                ..Default::default()
            },
            Some(Duration::from_secs(60)),
        )
        .await;

    assert!(
        result.is_ok(),
        "Gasless transaction should succeed via drive_transaction: {:?}",
        result.err()
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_disabled_rejects_transaction() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.disable_gasless_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(1);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx = test_env.create_gasless_transaction(1000, coin_type, sender, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Gas price"),
        "Expected gas price validation error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_computation_cap() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_gasless_for_testing();
            cfg.set_gasless_max_computation_units_for_testing(1);
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx = test_env.create_gasless_transaction(100, coin_type, sender, recipient, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Gasless should fail when computation exceeds cap"
    );
    assert_zero_gas(effects.gas_cost_summary());

    test_env.trigger_reconfiguration().await;
}
