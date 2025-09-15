// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_macros::sim_test;
use sui_rpc::client::v2::Client;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_sdk_types::Address;
use sui_types::gas_coin::GAS;
use sui_types::TypeTag;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;

use crate::{stake_with_validator, transfer_coin};

fn calculate_mints_and_burns(
    balance_changes: &[sui_types::balance_change::BalanceChange],
) -> BTreeMap<TypeTag, i128> {
    let mut totals = BTreeMap::new();
    for change in balance_changes {
        *totals.entry(change.coin_type.clone()).or_insert(0) += change.amount;
    }

    totals.retain(|_key, value| value != &0);

    totals
}

fn assert_balance_conservation(
    balance_changes: &[sui_types::balance_change::BalanceChange],
    coin_type: &TypeTag,
) {
    let mints_and_burns = calculate_mints_and_burns(balance_changes);
    if let Some(amount) = mints_and_burns.get(coin_type) {
        panic!(
            "{} not conserved: {amount}",
            coin_type.to_canonical_display(true)
        );
    }
}

#[sim_test]
async fn test_balance_changes() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(5000)
        .build()
        .await;

    let mut client = Client::new(test_cluster.rpc_url()).unwrap();

    // Setup a checkpont subscription task and wait for an end of epoch txn and verify that sui is
    // conserved; and that balance changes are properly returned from the SubscribeCheckpoints api.
    let mut stream = client
        .subscription_client()
        .subscribe_checkpoints(SubscribeCheckpointsRequest::default().with_read_mask(
            FieldMask::from_str("summary,transactions.transaction,transactions.balance_changes"),
        ))
        .await
        .unwrap()
        .into_inner();

    let verify_end_of_epoch_task = tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            let checkpoint = item.unwrap().checkpoint.unwrap();
            // Wait for an eoe checkpoint
            if checkpoint.summary().end_of_epoch_data_opt().is_some() {
                let eoe = checkpoint
                    .transactions()
                    .iter()
                    .find(|t| {
                        t.transaction().kind().kind()
                            == sui_rpc::proto::sui::rpc::v2::transaction_kind::Kind::EndOfEpoch
                    })
                    .unwrap();
                let balance_changes = eoe
                    .balance_changes()
                    .iter()
                    .map(sui_types::balance_change::BalanceChange::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();

                assert!(!balance_changes.is_empty());
                assert_balance_conservation(&balance_changes, &GAS::type_().into());

                // Only exit after we've verified an eoe checkpoint for an epoch that had some txns
                // executed in it
                if checkpoint
                    .summary()
                    .epoch_rolling_gas_cost_summary()
                    .computation_cost()
                    != 0
                {
                    break;
                }
            }
        }
    });

    let transfer_digest = transfer_coin(&test_cluster.wallet).await;
    let stake_digest = stake_with_validator(&test_cluster).await;

    // Verify genesis checkpoint mints TOTAL_SUPPLY_MIST and that GetCheckpoint properly returns
    // balance changes.
    let genesis = client
        .ledger_client()
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(0u64).with_read_mask(FieldMask::from_str(
                "transactions.digest,transactions.balance_changes",
            )),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let balance_changes = genesis.transactions()[0]
        .balance_changes()
        .iter()
        .map(sui_types::balance_change::BalanceChange::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!balance_changes.is_empty());
    let mints_and_burns = calculate_mints_and_burns(&balance_changes);
    assert_eq!(
        *mints_and_burns.get(&GAS::type_().into()).unwrap(),
        sui_types::gas_coin::TOTAL_SUPPLY_MIST as i128
    );

    // Verify that the only balance change in the staking txn is the gas paid
    let stake_txn = client
        .ledger_client()
        .get_transaction(
            GetTransactionRequest::new(&stake_digest)
                .with_read_mask(FieldMask::from_str("transaction,effects,balance_changes")),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    let net_gas = stake_txn.effects().gas_used().computation_cost() as i64
        + stake_txn.effects().gas_used().storage_cost() as i64
        - stake_txn.effects().gas_used().storage_rebate() as i64;
    let balance_changes = stake_txn
        .balance_changes()
        .iter()
        .map(sui_types::balance_change::BalanceChange::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_balance_conservation(&balance_changes, &GAS::type_().into());
    assert!(balance_changes
        .iter()
        .all(|b| b.amount.abs() == net_gas.abs() as i128));

    // Verify that the gas paid in the transfer is paid to 0x5
    let stake_txn = client
        .ledger_client()
        .get_transaction(
            GetTransactionRequest::new(&transfer_digest)
                .with_read_mask(FieldMask::from_str("transaction,effects,balance_changes")),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    let net_gas = stake_txn.effects().gas_used().computation_cost() as i64
        + stake_txn.effects().gas_used().storage_cost() as i64
        - stake_txn.effects().gas_used().storage_rebate() as i64;
    let balance_changes = stake_txn
        .balance_changes()
        .iter()
        .map(sui_types::balance_change::BalanceChange::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_balance_conservation(&balance_changes, &GAS::type_().into());
    assert_eq!(
        balance_changes
            .iter()
            .find(|b| b.address == Address::from_hex_unwrap("0x5").into())
            .unwrap()
            .amount,
        net_gas as i128
    );

    // Join on the end_of_epoch task and verify it exited successfully
    verify_end_of_epoch_task.await.unwrap();
}

#[sim_test]
async fn test_wrapped_balance_changes_with_dynamic_child_object() {
    todo!()
}

// TODO
// - recieving
// - wrapping
// - dynamic field with wrapping?
