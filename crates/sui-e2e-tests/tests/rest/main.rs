// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod checkpoints;
mod committee;
mod execute;
mod objects;
mod resolve;
mod transactions;

async fn transfer_coin(
    context: &sui_sdk::wallet_context::WalletContext,
) -> sui_sdk_types::types::TransactionDigest {
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
