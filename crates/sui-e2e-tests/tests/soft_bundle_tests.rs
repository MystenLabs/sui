// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::messages_grpc::{RawSubmitTxRequest, SubmitTxType};
use test_cluster::TestClusterBuilder;

/// Soft bundles require all transactions to use the same gas price.
#[sim_test]
async fn test_soft_bundle_rejects_mixed_gas_prices() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![AccountConfig {
            address: None,
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; 2],
        }])
        .build()
        .await;

    let accounts_and_gas = test_cluster
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let sender = accounts_and_gas[0].0;
    let gas_coins = &accounts_and_gas[0].1;
    let rgp = test_cluster.get_reference_gas_price().await;

    let tx1 = TestTransactionBuilder::new(sender, gas_coins[0], rgp)
        .transfer_sui(Some(1), sender)
        .build();
    let signed_tx1 = test_cluster.wallet.sign_transaction(&tx1).await;

    let tx2 = TestTransactionBuilder::new(sender, gas_coins[1], rgp * 2)
        .transfer_sui(Some(1), sender)
        .build();
    let signed_tx2 = test_cluster.wallet.sign_transaction(&tx2).await;

    let mut validator_client = test_cluster
        .authority_aggregator()
        .authority_clients
        .iter()
        .next()
        .unwrap()
        .1
        .authority_client()
        .get_client_for_testing()
        .unwrap();

    let request = RawSubmitTxRequest {
        transactions: vec![
            bcs::to_bytes(&signed_tx1).unwrap().into(),
            bcs::to_bytes(&signed_tx2).unwrap().into(),
        ],
        submit_type: SubmitTxType::SoftBundle.into(),
    };

    let error = validator_client
        .submit_transaction(request)
        .await
        .expect_err("soft bundle with mixed gas prices must be rejected");

    assert!(
        error.to_string().contains("Gas price for transaction")
            && error.to_string().contains("in Soft Bundle mismatch"),
        "expected gas price mismatch error, got: {error}"
    );
}
