// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use sui_macros::sim_test;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::rpc::v2beta::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_rpc_api::proto::rpc::v2beta::Bcs;
use sui_rpc_api::proto::rpc::v2beta::ExecuteTransactionRequest;
use sui_rpc_api::proto::rpc::v2beta::ExecuteTransactionResponse;
use sui_rpc_api::proto::rpc::v2beta::Transaction;
use sui_rpc_api::proto::rpc::v2beta::UserSignature;
use sui_sdk_types::BalanceChange;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn execute_transaction_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let address = SuiAddress::random_for_testing_only();
    let amount = 9;

    let txn =
        make_transfer_sui_transaction(&test_cluster.wallet, Some(address), Some(amount)).await;
    let sender = txn.transaction_data().sender();

    let ExecuteTransactionResponse {
        finality: _,
        transaction,
    } = client
        .execute_transaction(ExecuteTransactionRequest {
            transaction: Some(Transaction {
                bcs: Some(Bcs::serialize(txn.transaction_data()).unwrap()),
                ..Default::default()
            }),
            signatures: txn
                .tx_signatures()
                .iter()
                .map(|s| UserSignature {
                    bcs: Some(Bcs {
                        name: None,
                        value: Some(s.as_ref().to_owned().into()),
                    }),
                    ..Default::default()
                })
                .collect(),
            read_mask: Some(FieldMask::from_paths(["finality", "transaction"])),
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();
    let gas_summary =
        sui_sdk_types::GasCostSummary::try_from(&transaction.effects.unwrap().gas_used.unwrap())
            .unwrap();
    let gas = gas_summary.net_gas_usage();

    let coin_type = sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk(
        sui_types::gas_coin::GAS::type_tag(),
    )
    .unwrap();
    let mut expected = vec![
        BalanceChange {
            address: sender.into(),
            coin_type: coin_type.clone(),
            amount: -(amount as i128 + gas as i128),
        },
        BalanceChange {
            address: address.into(),
            coin_type,
            amount: amount as i128,
        },
    ];
    expected.sort_by_key(|e| e.address);

    let mut actual = transaction
        .balance_changes
        .into_iter()
        .map(|bc| BalanceChange::try_from(&bc).unwrap())
        .collect::<Vec<_>>();
    actual.sort_by_key(|e| e.address);

    assert_eq!(actual, expected);
}
