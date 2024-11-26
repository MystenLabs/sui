// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::init_state_with_ids_and_object_basics;
use bcs;
use sui_types::{
    execution_status::ExecutionStatus,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    utils::to_sender_signed_transaction,
};

use authority_tests::send_and_confirm_transaction;
use move_core_types::{account_address::AccountAddress, ident_str};
use sui_types::{
    crypto::{get_key_pair, AccountKeyPair},
    object::Owner,
};

#[tokio::test]
async fn test_batch_transaction_ok() -> anyhow::Result<()> {
    // This test tests a sucecssful normal batch transaction.
    // This batch transaction contains 5 transfers, and 5 Move calls.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, _): (_, AccountKeyPair) = get_key_pair();
    const N: usize = 5;
    const TOTAL: usize = N + 1;
    let all_ids = (0..TOTAL).map(|_| ObjectID::random()).collect::<Vec<_>>();
    let (authority_state, package) = init_state_with_ids_and_object_basics(
        [sender; TOTAL].into_iter().zip(all_ids.clone().into_iter()),
    )
    .await;
    let rgp = authority_state.reference_gas_price_for_testing()?;
    let mut builder = ProgrammableTransactionBuilder::new();
    for obj_id in all_ids.iter().take(N) {
        builder
            .transfer_object(
                recipient,
                authority_state
                    .get_object(obj_id)
                    .await
                    .unwrap()
                    .compute_object_reference(),
            )
            .unwrap()
    }
    for _ in 0..N {
        builder
            .move_call(
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![
                    CallArg::Pure(16u64.to_le_bytes().to_vec()),
                    CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
                ],
            )
            .unwrap();
    }
    let data = TransactionData::new_programmable(
        sender,
        vec![authority_state
            .get_object(&all_ids[N])
            .await
            .unwrap()
            .compute_object_reference()],
        builder.finish(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * (N as u64),
        rgp,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await?;
    let effects = response.1.into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
    assert_eq!(
        (effects.created().len(), effects.mutated().len()),
        (N, N + 1),
    );
    assert!(effects
        .created()
        .iter()
        .all(|(_, owner)| owner == &Owner::AddressOwner(sender)));
    // N of the objects should now be owned by recipient.
    assert_eq!(
        effects
            .mutated()
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
    const N: usize = 5;
    const TOTAL: usize = N + 1;
    let all_ids = (0..TOTAL).map(|_| ObjectID::random()).collect::<Vec<_>>();
    let (authority_state, package) = init_state_with_ids_and_object_basics(
        [sender; TOTAL].into_iter().zip(all_ids.clone().into_iter()),
    )
    .await;
    let rgp = authority_state.reference_gas_price_for_testing()?;
    let mut builder = ProgrammableTransactionBuilder::new();
    for obj_id in all_ids.iter().take(N) {
        builder
            .transfer_object(
                recipient,
                authority_state
                    .get_object(obj_id)
                    .await
                    .unwrap()
                    .compute_object_reference(),
            )
            .unwrap()
    }
    builder
        .move_call(
            package.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("create").to_owned(),
            vec![],
            vec![],
        )
        .unwrap();
    let data = TransactionData::new_programmable(
        sender,
        vec![authority_state
            .get_object(&all_ids[N])
            .await
            .unwrap()
            .compute_object_reference()],
        builder.finish(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        rgp,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);

    let response = send_and_confirm_transaction(&authority_state, tx).await?.1;
    let effects = response.into_data();
    assert!(effects.status().is_err());
    assert_eq!(
        (effects.created().len(), effects.mutated().len()),
        (0, N + 1)
    );

    Ok(())
}

#[tokio::test]
async fn test_batch_insufficient_gas_balance() -> anyhow::Result<()> {
    // This test creates 10 Move call transactions batch, each with a budget of 5000.
    // However we provide a gas coin with only 49999 balance.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (authority_state, package) = init_state_with_ids_and_object_basics([]).await;
    let rgp = authority_state.reference_gas_price_for_testing()?;
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_for_testing(
        gas_object_id,
        sender,
        49999, // We need 50000
    );
    authority_state
        .insert_genesis_object(gas_object.clone())
        .await;

    const N: usize = 10;
    let mut builder = ProgrammableTransactionBuilder::new();
    for _ in 0..N {
        builder
            .move_call(
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("create").to_owned(),
                vec![],
                vec![
                    CallArg::Pure(16u64.to_le_bytes().to_vec()),
                    CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
                ],
            )
            .unwrap();
    }
    let data = TransactionData::new_programmable(
        sender,
        vec![gas_object.compute_object_reference()],
        builder.finish(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        rgp,
    );

    let tx = to_sender_signed_transaction(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, tx).await;

    assert!(matches!(
        UserInputError::try_from(response.unwrap_err()).unwrap(),
        UserInputError::GasBalanceTooLow { .. }
    ));

    Ok(())
}
