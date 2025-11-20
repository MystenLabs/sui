// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::*;
use sui_types::{
    digests::{ChainIdentifier, CheckpointDigest},
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        Argument, Command, GasData, TransactionData, TransactionDataV1, TransactionExpiration,
        TransactionKind,
    },
};
use test_cluster::TestClusterBuilder;

async fn execute_txn_with_expiration(
    test_cluster: &test_cluster::TestCluster,
    min_epoch: Option<u64>,
    max_epoch: Option<u64>,
    chain_id_override: Option<ChainIdentifier>,
) -> Result<
    (
        sui_types::effects::TransactionEffects,
        sui_types::effects::TransactionEvents,
    ),
    anyhow::Error,
> {
    let (sender, gas_coin) = test_cluster.get_one_sender_and_gas().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = chain_id_override.unwrap_or_else(|| test_cluster.get_chain_identifier());

    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(1000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };
    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);
    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx_data = TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch,
            max_epoch,
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce: 12345,
        },
    });

    let signed_tx = test_cluster.sign_transaction(&tx_data).await;
    test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
}

async fn setup_test_cluster() -> test_cluster::TestCluster {
    TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await
}

fn assert_error_contains(
    result: Result<impl std::fmt::Debug, impl std::fmt::Debug>,
    expected_error: &str,
    panic_message: &str,
) {
    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains(expected_error),
                "Expected {} error, got: {:?}",
                expected_error,
                err
            );
        }
        Ok(_) => panic!("{}", panic_message),
    }
}

#[sim_test]
async fn test_regular_gas_payment_with_valid_during_current_epoch() {
    let test_cluster = setup_test_cluster().await;
    let current_epoch = 0;

    let (effects, _) = execute_txn_with_expiration(
        &test_cluster,
        Some(current_epoch),
        Some(current_epoch),
        None,
    )
    .await
    .unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should execute successfully. Error: {:?}",
        effects.status()
    );
}

#[sim_test]
async fn test_transaction_expired_too_early() {
    let test_cluster = setup_test_cluster().await;
    let future_epoch = 10;

    let result =
        execute_txn_with_expiration(&test_cluster, Some(future_epoch), Some(future_epoch), None)
            .await;

    assert_error_contains(
        result,
        "TransactionExpired",
        "Transaction should be rejected when epoch is too early",
    );
}

#[sim_test]
async fn test_transaction_expired_too_late() {
    let test_cluster = setup_test_cluster().await;
    let past_epoch = 0;

    test_cluster.trigger_reconfiguration().await;

    let result =
        execute_txn_with_expiration(&test_cluster, Some(past_epoch), Some(past_epoch), None).await;

    assert_error_contains(
        result,
        "TransactionExpired",
        "Transaction should be rejected when epoch is too late",
    );
}

#[sim_test]
async fn test_transaction_invalid_chain_id() {
    let test_cluster = setup_test_cluster().await;
    let current_epoch = 0;
    let wrong_chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let result = execute_txn_with_expiration(
        &test_cluster,
        Some(current_epoch),
        Some(current_epoch),
        Some(wrong_chain_id),
    )
    .await;

    assert_error_contains(
        result,
        "InvalidChainId",
        "Transaction should be rejected with invalid chain ID",
    );
}

#[sim_test]
async fn test_transaction_expiration_min_none_max_some() {
    let test_cluster = setup_test_cluster().await;
    let current_epoch = 0;

    let result =
        execute_txn_with_expiration(&test_cluster, None, Some(current_epoch + 5), None).await;

    assert_error_contains(
        result,
        "Both min_epoch and max_epoch must be specified and equal",
        "Transaction should be rejected when only max_epoch is specified",
    );
}

#[sim_test]
async fn test_transaction_with_current_epoch_succeeds() {
    let test_cluster = setup_test_cluster().await;
    let current_epoch = 0;

    let result = execute_txn_with_expiration(
        &test_cluster,
        Some(current_epoch),
        Some(current_epoch),
        None,
    )
    .await;

    assert!(
        result.is_ok(),
        "Single epoch window transaction should succeed"
    );
}

#[sim_test]
async fn test_transaction_with_future_epoch_fails() {
    let test_cluster = setup_test_cluster().await;
    let current_epoch = 0;

    let result = execute_txn_with_expiration(
        &test_cluster,
        Some(current_epoch + 1),
        Some(current_epoch + 1),
        None,
    )
    .await;

    assert_error_contains(
        result,
        "TransactionExpired",
        "Transaction should be rejected when min_epoch is in the future",
    );
}

#[sim_test]
async fn test_transaction_with_past_epoch_fails() {
    let test_cluster = setup_test_cluster().await;

    test_cluster.trigger_reconfiguration().await;

    let result = execute_txn_with_expiration(&test_cluster, Some(0), Some(0), None).await;

    assert_error_contains(
        result,
        "TransactionExpired",
        "Transaction should be rejected when max_epoch is in the past",
    );
}
