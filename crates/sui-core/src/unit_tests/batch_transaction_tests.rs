// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::init_state_with_ids_and_object_basics;
use bcs;
use sui_types::utils::to_sender_signed_transaction;

use authority_tests::{init_state_with_ids, send_and_confirm_transaction};
use move_binary_format::file_format;
use move_core_types::{account_address::AccountAddress, ident_str};
use sui_types::{
    crypto::{get_key_pair, AccountKeyPair},
    object::Owner,
};

#[tokio::test]
async fn test_batch_transaction_ok() -> anyhow::Result<()> {
    // This test tests a sucecssful normal batch transaction.
    // This batch transaction contains 100 transfers, and 100 Move calls.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, _): (_, AccountKeyPair) = get_key_pair();
    const N: usize = 10;
    const TOTAL: usize = N + 1;
    let all_ids = (0..TOTAL).map(|_| ObjectID::random()).collect::<Vec<_>>();
    let (authority_state, package) = init_state_with_ids_and_object_basics(
        [sender; TOTAL].into_iter().zip(all_ids.clone().into_iter()),
    )
    .await;
    let mut transactions = vec![];
    for obj_id in all_ids.iter().take(N) {
        transactions.push(SingleTransactionKind::TransferObject(TransferObject {
            recipient,
            object_ref: authority_state
                .get_object(obj_id)
                .await?
                .unwrap()
                .compute_object_reference(),
        }));
    }
    for _ in 0..N {
        transactions.push(SingleTransactionKind::Call(MoveCall {
            package: package.0,
            module: ident_str!("object_basics").to_owned(),
            function: ident_str!("create").to_owned(),
            type_arguments: vec![],
            arguments: vec![
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
        }));
    }
    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Batch(transactions),
        sender,
        authority_state
            .get_object(&all_ids[N])
            .await?
            .unwrap()
            .compute_object_reference(),
        1000000,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await?;
    let effects = response.1.into_data();
    assert!(effects.status.is_ok());
    assert_eq!((effects.created.len(), effects.mutated.len()), (N, N + 1),);
    assert!(effects
        .created
        .iter()
        .all(|(_, owner)| owner == &Owner::AddressOwner(sender)));
    // N of the objects should now be owned by recipient.
    assert_eq!(
        effects
            .mutated
            .iter()
            .filter(|(_, owner)| owner == &Owner::AddressOwner(recipient))
            .count(),
        N
    );

    Ok(())
}

#[tokio::test]
async fn test_batch_transaction_last_one_fail() -> anyhow::Result<()> {
    // This test tests the case where the last transaction in a batch transaction would fail to execute.
    // We make sure that the entire batch is rolled back, and only gas is charged.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, _): (_, AccountKeyPair) = get_key_pair();
    const N: usize = 100;
    const TOTAL: usize = N + 1;
    let all_ids = (0..TOTAL).map(|_| ObjectID::random()).collect::<Vec<_>>();
    let (authority_state, package) = init_state_with_ids_and_object_basics(
        [sender; TOTAL].into_iter().zip(all_ids.clone().into_iter()),
    )
    .await;
    let mut transactions = vec![];
    for obj_id in all_ids.iter().take(N) {
        transactions.push(SingleTransactionKind::TransferObject(TransferObject {
            recipient,
            object_ref: authority_state
                .get_object(obj_id)
                .await?
                .unwrap()
                .compute_object_reference(),
        }));
    }
    transactions.push(SingleTransactionKind::Call(MoveCall {
        package: package.0,
        module: ident_str!("object_basics").to_owned(),
        function: ident_str!("create").to_owned(),
        type_arguments: vec![],
        arguments: vec![],
    }));
    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Batch(transactions),
        sender,
        authority_state
            .get_object(&all_ids[N])
            .await?
            .unwrap()
            .compute_object_reference(),
        100000,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);

    let response = send_and_confirm_transaction(&authority_state, tx).await?.1;
    let effects = response.into_data();
    assert!(effects.status.is_err());
    assert_eq!((effects.created.len(), effects.mutated.len()), (0, N + 1));

    Ok(())
}

#[tokio::test]
async fn test_batch_contains_publish() -> anyhow::Result<()> {
    // Test that a batch transaction containing publish will fail.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids([(sender, gas_object_id)]).await;
    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let transactions = vec![SingleTransactionKind::Publish(MoveModulePublish {
        modules: module_bytes,
    })];
    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Batch(transactions),
        sender,
        authority_state
            .get_object(&gas_object_id)
            .await?
            .unwrap()
            .compute_object_reference(),
        100000,
    );
    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await;
    assert!(matches!(
        *response
            .unwrap_err()
            .collapse_if_single_transaction_input_error()
            .unwrap(),
        SuiError::InvalidBatchTransaction { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn test_batch_contains_transfer_sui() -> anyhow::Result<()> {
    // Test that a batch transaction containing TransferSui will fail.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids([(sender, gas_object_id)]).await;
    let transactions = vec![SingleTransactionKind::TransferSui(TransferSui {
        recipient: Default::default(),
        amount: None,
    })];
    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Batch(transactions),
        sender,
        authority_state
            .get_object(&gas_object_id)
            .await?
            .unwrap()
            .compute_object_reference(),
        100000,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await;
    assert!(matches!(
        *response
            .unwrap_err()
            .collapse_if_single_transaction_input_error()
            .unwrap(),
        SuiError::InvalidBatchTransaction { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn test_batch_insufficient_gas_balance() -> anyhow::Result<()> {
    // This test creates 100 Move call transactions batch, each with a budget of 5000.
    // However we provide a gas coin with only 49999 balance.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (authority_state, package) = init_state_with_ids_and_object_basics([]).await;
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_for_testing(
        gas_object_id,
        sender,
        49999, // We need 50000
    );
    authority_state
        .insert_genesis_object(gas_object.clone())
        .await;

    const N: usize = 100;
    let mut transactions = vec![];
    for _ in 0..N {
        transactions.push(SingleTransactionKind::Call(MoveCall {
            package: package.0,
            module: ident_str!("object_basics").to_owned(),
            function: ident_str!("create").to_owned(),
            type_arguments: vec![],
            arguments: vec![
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
        }));
    }
    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Batch(transactions),
        sender,
        gas_object.compute_object_reference(),
        100000,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await;

    assert!(matches!(
        *response
            .unwrap_err()
            .collapse_if_single_transaction_input_error()
            .unwrap(),
        SuiError::GasBalanceTooLowToCoverGasBudget { .. }
    ));

    Ok(())
}
