// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod checkpoints;
mod coin_info;
mod committee;
mod execute;
mod node_info;
mod objects;
mod resolve;
mod transactions;

async fn transfer_coin(
    context: &sui_sdk::wallet_context::WalletContext,
) -> sui_sdk_types::TransactionDigest {
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];
    let txn = context.sign_transaction(
        &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, gas_price)
            .transfer(object_to_send, receiver)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.digest.into()
}

async fn stake_with_validator(
    cluster: &test_cluster::TestCluster,
) -> sui_sdk_types::TransactionDigest {
    let context = &cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let gas_object = accounts_and_objs[0].1[0];
    let coin_to_stake = accounts_and_objs[0].1[1];
    let validator_address = cluster.swarm.config().validator_configs()[0].sui_address();
    let txn = context.sign_transaction(
        &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, gas_price)
            .call_staking(coin_to_stake, validator_address)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.digest.into()
}
