// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// These tests use the JSON-RPC dry_run_transaction_block endpoint to verify
// the behavior that external clients observe when submitting transactions with
// invalid gas parameters.
#![allow(deprecated)]

use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::TransactionData,
};
use test_cluster::TestClusterBuilder;

async fn build_transfer_sui_tx(
    sender: SuiAddress,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(SuiAddress::random_for_testing_only(), None);
    let pt = builder.finish();
    // Empty gas payment — dry_run injects a mock gas coin.
    TransactionData::new_programmable(sender, vec![], pt, gas_budget, gas_price)
}

#[sim_test]
async fn test_dry_run_gas_budget_too_low() {
    let cluster = TestClusterBuilder::new().build().await;
    let client = cluster.sui_client();
    let sender = cluster.get_address_0();
    let rgp = cluster.get_reference_gas_price().await;

    let tx = build_transfer_sui_tx(sender, 0, rgp).await;
    let err = client
        .read_api()
        .dry_run_transaction_block(tx)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("is lower than min:"),
        "Expected GasBudgetTooLow error, got: {err}"
    );
}

#[sim_test]
async fn test_dry_run_gas_budget_too_high() {
    let cluster = TestClusterBuilder::new().build().await;
    let client = cluster.sui_client();
    let sender = cluster.get_address_0();
    let rgp = cluster.get_reference_gas_price().await;
    let max_budget = ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas();

    let tx = build_transfer_sui_tx(sender, max_budget + 1, rgp).await;
    let err = client
        .read_api()
        .dry_run_transaction_block(tx)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("is higher than max:"),
        "Expected GasBudgetTooHigh error, got: {err}"
    );
}

#[sim_test]
async fn test_dry_run_gas_price_too_low() {
    let cluster = TestClusterBuilder::new().build().await;
    let client = cluster.sui_client();
    let sender = cluster.get_address_0();
    let max_budget = ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas();
    let rgp = cluster.get_reference_gas_price().await;

    // Use rgp - 1, which is below RGP but above 0 so the transaction is not
    // treated as a gasless transaction (which requires price == 0).
    let tx = build_transfer_sui_tx(sender, max_budget, rgp - 1).await;
    let err = client
        .read_api()
        .dry_run_transaction_block(tx)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("under reference gas price (RGP)"),
        "Expected GasPriceUnderRGP error, got: {err}"
    );
}

#[sim_test]
async fn test_dry_run_gas_price_too_high() {
    let cluster = TestClusterBuilder::new().build().await;
    let client = cluster.sui_client();
    let sender = cluster.get_address_0();
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let max_gas_price = config.max_gas_price();
    let max_budget = config.max_tx_gas();

    // max_gas_price satisfies the too-high condition (price >= max_gas_price).
    let tx = build_transfer_sui_tx(sender, max_budget, max_gas_price).await;
    let err = client
        .read_api()
        .dry_run_transaction_block(tx)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Gas price cannot exceed"),
        "Expected GasPriceTooHigh error, got: {err}"
    );
}
