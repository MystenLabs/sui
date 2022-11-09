// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;

use crate::authority::authority_tests::{init_state, send_and_confirm_transaction};
use crate::authority::AuthorityState;
use futures::future::join_all;
use std::collections::HashMap;
use sui_types::crypto::AccountKeyPair;
use sui_types::{
    base_types::dbg_addr,
    crypto::{get_key_pair, Signature},
    error::SuiError,
    messages::{Transaction, TransactionInfoResponse, TransactionKind},
};

#[tokio::test]
async fn test_pay_sui_failure_empty_recipients() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);

    let res = execute_pay_sui(vec![coin1], vec![], vec![], sender, sender_key, 100).await;

    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(
        effects.status,
        ExecutionStatus::new_failure(ExecutionFailureStatus::EmptyRecipients)
    );
}

#[tokio::test]
async fn test_pay_sui_failure_arity_mismatch() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);

    let res = execute_pay_sui(
        vec![coin1],
        vec![recipient1, recipient2],
        vec![10],
        sender,
        sender_key,
        100,
    )
    .await;

    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(
        effects.status,
        ExecutionStatus::new_failure(ExecutionFailureStatus::RecipientsAmountsArityMismatch)
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_gas_balance_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        1200,
    )
    .await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                1000, 1200, 1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_total_balance_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        900,
    )
    .await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Total balance is {}, not enough to pay {} with gas price of {}",
                1000,
                100 + 100 + 900,
                1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_gas_balance_multiple_input_coins() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 400);
    let coin2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 600);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1, coin2],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        801,
    )
    .await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                400, 801, 1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_total_balance_multiple_input_coins() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 400);
    let coin2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 600);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1, coin2],
        vec![recipient1, recipient2],
        vec![400, 400],
        sender,
        sender_key,
        201,
    )
    .await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Total balance is {}, not enough to pay {} with gas price of {}",
                400 + 600,
                400 + 400 + 201,
                1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_sui_success_one_input_coin() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id = ObjectID::random();
    let coin_obj = Object::with_id_owner_gas_for_testing(object_id, sender, 2000);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);
    let recipient3 = dbg_addr(3);
    let recipient_amount_map: HashMap<_, u64> =
        HashMap::from([(recipient1, 100), (recipient2, 200), (recipient3, 300)]);
    let res = execute_pay_sui(
        vec![coin_obj],
        vec![recipient1, recipient2, recipient3],
        vec![100, 200, 300],
        sender,
        sender_key,
        1000,
    )
    .await;

    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(effects.status, ExecutionStatus::Success);
    // make sure each recipient receives the specified amount
    assert_eq!(effects.created.len(), 3);
    let created_obj_id1 = effects.created[0].0 .0;
    let created_obj_id2 = effects.created[1].0 .0;
    let created_obj_id3 = effects.created[2].0 .0;
    let created_obj1 = res
        .authority_state
        .get_object(&created_obj_id1)
        .await
        .unwrap()
        .unwrap();
    let created_obj2 = res
        .authority_state
        .get_object(&created_obj_id2)
        .await
        .unwrap()
        .unwrap();
    let created_obj3 = res
        .authority_state
        .get_object(&created_obj_id3)
        .await
        .unwrap()
        .unwrap();

    let addr1 = effects.created[0].1.get_owner_address()?;
    let addr2 = effects.created[1].1.get_owner_address()?;
    let addr3 = effects.created[2].1.get_owner_address()?;
    let coin_val1 = *recipient_amount_map
        .get(&addr1)
        .ok_or(SuiError::InvalidAddress)?;
    let coin_val2 = *recipient_amount_map
        .get(&addr2)
        .ok_or(SuiError::InvalidAddress)?;
    let coin_val3 = *recipient_amount_map
        .get(&addr3)
        .ok_or(SuiError::InvalidAddress)?;
    assert_eq!(GasCoin::try_from(&created_obj1)?.value(), coin_val1);
    assert_eq!(GasCoin::try_from(&created_obj2)?.value(), coin_val2);
    assert_eq!(GasCoin::try_from(&created_obj3)?.value(), coin_val3);

    // make sure the first object still belongs to the sender,
    // the value is equal to all residual values after amounts transferred and gas payment.
    assert_eq!(effects.mutated[0].0 .0, object_id);
    assert_eq!(effects.mutated[0].1, sender);
    let gas_used = effects.gas_used.gas_used();
    let gas_object = res.authority_state.get_object(&object_id).await?.unwrap();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        2000 - 100 - 200 - 300 - gas_used,
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_sui_success_multiple_input_coins() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id1 = ObjectID::random();
    let object_id2 = ObjectID::random();
    let object_id3 = ObjectID::random();
    let coin_obj1 = Object::with_id_owner_gas_for_testing(object_id1, sender, 1000);
    let coin_obj2 = Object::with_id_owner_gas_for_testing(object_id2, sender, 1000);
    let coin_obj3 = Object::with_id_owner_gas_for_testing(object_id3, sender, 1000);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin_obj1, coin_obj2, coin_obj3],
        vec![recipient1, recipient2],
        vec![500, 1500],
        sender,
        sender_key,
        1000,
    )
    .await;
    let recipient_amount_map: HashMap<_, u64> =
        HashMap::from([(recipient1, 500), (recipient2, 1500)]);
    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(effects.status, ExecutionStatus::Success);

    // make sure each recipient receives the specified amount
    assert_eq!(effects.created.len(), 2);
    let created_obj_id1 = effects.created[0].0 .0;
    let created_obj_id2 = effects.created[1].0 .0;
    let created_obj1 = res
        .authority_state
        .get_object(&created_obj_id1)
        .await
        .unwrap()
        .unwrap();
    let created_obj2 = res
        .authority_state
        .get_object(&created_obj_id2)
        .await
        .unwrap()
        .unwrap();
    let addr1 = effects.created[0].1.get_owner_address()?;
    let addr2 = effects.created[1].1.get_owner_address()?;
    let coin_val1 = *recipient_amount_map
        .get(&addr1)
        .ok_or(SuiError::InvalidAddress)?;
    let coin_val2 = *recipient_amount_map
        .get(&addr2)
        .ok_or(SuiError::InvalidAddress)?;
    assert_eq!(GasCoin::try_from(&created_obj1)?.value(), coin_val1);
    assert_eq!(GasCoin::try_from(&created_obj2)?.value(), coin_val2);
    // make sure the first input coin still belongs to the sender,
    // the value is equal to all residual values after amounts transferred and gas payment.
    assert_eq!(effects.mutated[0].0 .0, object_id1);
    assert_eq!(effects.mutated[0].1, sender);
    let gas_used = effects.gas_used.gas_used();
    let gas_object = res.authority_state.get_object(&object_id1).await?.unwrap();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        3000 - 500 - 1500 - gas_used,
    );

    // make sure the second and third input coins are deleted
    let deleted_ids: Vec<ObjectID> = effects.deleted.iter().map(|d| d.0).collect();
    assert!(deleted_ids.contains(&object_id2));
    assert!(deleted_ids.contains(&object_id3));
    Ok(())
}

#[tokio::test]
async fn test_pay_all_sui_failure_insufficient_gas_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient = dbg_addr(2);

    let res = execute_pay_all_sui(vec![&coin1], recipient, sender, sender_key, 2000).await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                1000, 2000, 1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_all_sui_failure_insufficient_gas_budget_multiple_input_coins() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let coin2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient = dbg_addr(2);
    let res = execute_pay_all_sui(vec![&coin1, &coin2], recipient, sender, sender_key, 2500).await;

    let err = res.txn_result.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                1000, 2500, 1
            )
        }
    );
}

#[tokio::test]
async fn test_pay_all_sui_success_one_input_coin() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id = ObjectID::random();
    let coin_obj = Object::with_id_owner_gas_for_testing(object_id, sender, 2000);
    let recipient = dbg_addr(2);
    let res = execute_pay_all_sui(vec![&coin_obj], recipient, sender, sender_key, 1000).await;

    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(effects.status, ExecutionStatus::Success);

    // make sure the first object now belongs to the recipient,
    // the value is equal to all residual values after gas payment.
    let obj_ref = &effects.mutated[0].0;
    assert_eq!(obj_ref.0, object_id);
    assert_eq!(effects.mutated[0].1, recipient);

    let gas_used = effects.gas_used.gas_used();
    let gas_object = res.authority_state.get_object(&object_id).await?.unwrap();
    assert_eq!(GasCoin::try_from(&gas_object)?.value(), 2000 - gas_used,);
    Ok(())
}

#[tokio::test]
async fn test_pay_all_sui_success_multiple_input_coins() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id1 = ObjectID::random();
    let coin_obj1 = Object::with_id_owner_gas_for_testing(object_id1, sender, 2000);
    let coin_obj2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let coin_obj3 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient = dbg_addr(2);
    let res = execute_pay_all_sui(
        vec![&coin_obj1, &coin_obj2, &coin_obj3],
        recipient,
        sender,
        sender_key,
        1000,
    )
    .await;

    let effects = res.txn_result.unwrap().signed_effects.unwrap().into_data();
    assert_eq!(effects.status, ExecutionStatus::Success);

    // make sure the first object now belongs to the recipient,
    // the value is equal to all residual values after gas payment.
    let obj_ref = &effects.mutated[0].0;
    assert_eq!(obj_ref.0, object_id1);
    assert_eq!(effects.mutated[0].1, recipient);

    let gas_used = effects.gas_used.gas_used();
    let gas_object = res.authority_state.get_object(&object_id1).await?.unwrap();
    assert_eq!(GasCoin::try_from(&gas_object)?.value(), 4000 - gas_used,);
    Ok(())
}

struct PaySuiTransactionExecutionResult {
    pub authority_state: AuthorityState,
    pub txn_result: Result<TransactionInfoResponse, SuiError>,
}

async fn execute_pay_sui(
    input_coin_objects: Vec<Object>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_budget: u64,
) -> PaySuiTransactionExecutionResult {
    let authority_state = init_state().await;

    let input_coin_refs: Vec<ObjectRef> = input_coin_objects
        .iter()
        .map(|coin_obj| coin_obj.compute_object_reference())
        .collect();
    let handles: Vec<_> = input_coin_objects
        .into_iter()
        .map(|obj| authority_state.insert_genesis_object(obj))
        .collect();
    join_all(handles).await;
    let gas_object_ref = input_coin_refs[0];

    let kind = TransactionKind::Single(SingleTransactionKind::PaySui(PaySui {
        coins: input_coin_refs,
        recipients,
        amounts,
    }));
    let data = TransactionData::new_with_gas_price(kind, sender, gas_object_ref, gas_budget, 1);

    let signature = Signature::new(&data, &sender_key);
    let tx = Transaction::from_data(data, signature).verify().unwrap();
    let txn_result = send_and_confirm_transaction(&authority_state, tx)
        .await
        .map(|t| t.into());

    PaySuiTransactionExecutionResult {
        authority_state,
        txn_result,
    }
}

async fn execute_pay_all_sui(
    input_coin_objects: Vec<&Object>,
    recipient: SuiAddress,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_budget: u64,
) -> PaySuiTransactionExecutionResult {
    let authority_state = init_state().await;
    authority_state
        .insert_genesis_objects_bulk_unsafe(&input_coin_objects)
        .await;

    let input_coins: Vec<ObjectRef> = input_coin_objects
        .iter()
        .map(|obj| obj.compute_object_reference())
        .collect();
    let gas_object_ref = input_coins[0];

    let kind = TransactionKind::Single(SingleTransactionKind::PayAllSui(PayAllSui {
        coins: input_coins,
        recipient,
    }));
    let data = TransactionData::new_with_gas_price(kind, sender, gas_object_ref, gas_budget, 1);
    let signature = Signature::new(&data, &sender_key);
    let tx = Transaction::from_data(data, signature).verify().unwrap();

    let txn_result = send_and_confirm_transaction(&authority_state, tx)
        .await
        .map(|t| t.into());

    PaySuiTransactionExecutionResult {
        authority_state,
        txn_result,
    }
}
