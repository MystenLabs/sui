// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_tests::{init_state_with_committee, send_and_confirm_transaction};
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::AuthorityState;
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::{SignedTransactionEffects, TransactionEffectsAPI};
use sui_types::error::UserInputError;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::TransactionData;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{base_types::dbg_addr, crypto::get_key_pair, error::SuiError};

#[tokio::test]
async fn test_pay_sui_failure_empty_recipients() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin_id = ObjectID::random();
    let coin1 = Object::with_id_owner_gas_for_testing(coin_id, sender, 2000000);

    // an empty set of programmable transaction commands will still charge gas
    let res = execute_pay_sui(vec![coin1], vec![], vec![], sender, sender_key, 2000000).await;

    let effects = res.txn_result.unwrap().into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
    assert_eq!(effects.mutated().len(), 1);
    assert_eq!(effects.mutated()[0].0 .0, coin_id);
    assert!(effects.deleted().is_empty());
    assert!(effects.created().is_empty());
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_gas_balance_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 2000);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        2200,
    )
    .await;

    assert_eq!(
        UserInputError::try_from(res.txn_result.unwrap_err()).unwrap(),
        UserInputError::GasBalanceTooLow {
            gas_balance: 2000,
            needed_gas_amount: 2200,
        }
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_total_balance_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 2600);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        2500,
    )
    .await;

    assert_eq!(
        res.txn_result.as_ref().unwrap().status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientCoinBalance,
            command: Some(0) // SplitCoins is the first command in the implementation of pay
        },
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_gas_balance_multiple_input_coins() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 800);
    let coin2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 700);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1, coin2],
        vec![recipient1, recipient2],
        vec![100, 100],
        sender,
        sender_key,
        2000,
    )
    .await;

    assert_eq!(
        UserInputError::try_from(res.txn_result.unwrap_err()).unwrap(),
        UserInputError::GasBalanceTooLow {
            gas_balance: 1500,
            needed_gas_amount: 2000,
        }
    );
}

#[tokio::test]
async fn test_pay_sui_failure_insufficient_total_balance_multiple_input_coins() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1400);
    let coin2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1300);
    let recipient1 = dbg_addr(1);
    let recipient2 = dbg_addr(2);

    let res = execute_pay_sui(
        vec![coin1, coin2],
        vec![recipient1, recipient2],
        vec![400, 400],
        sender,
        sender_key,
        2000,
    )
    .await;
    assert_eq!(
        res.txn_result.as_ref().unwrap().status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientCoinBalance,
            command: Some(0) // SplitCoins is the first command in the implementation of pay
        },
    );
}

#[tokio::test]
async fn test_pay_sui_success_one_input_coin() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id = ObjectID::random();
    let coin_obj = Object::with_id_owner_gas_for_testing(object_id, sender, 5000000);
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
        4000000,
    )
    .await;

    let effects = res.txn_result.unwrap().into_data();
    assert_eq!(*effects.status(), ExecutionStatus::Success);
    // make sure each recipient receives the specified amount
    assert_eq!(effects.created().len(), 3);
    let created_obj_id1 = effects.created()[0].0 .0;
    let created_obj_id2 = effects.created()[1].0 .0;
    let created_obj_id3 = effects.created()[2].0 .0;
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

    let addr1 = effects.created()[0].1.get_owner_address()?;
    let addr2 = effects.created()[1].1.get_owner_address()?;
    let addr3 = effects.created()[2].1.get_owner_address()?;
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
    assert_eq!(effects.mutated()[0].0 .0, object_id);
    assert_eq!(effects.mutated()[0].1, sender);
    let gas_used = effects.gas_cost_summary().net_gas_usage() as u64;
    let gas_object = res.authority_state.get_object(&object_id).await?.unwrap();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        5000000 - 100 - 200 - 300 - gas_used,
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_sui_success_multiple_input_coins() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id1 = ObjectID::random();
    let object_id2 = ObjectID::random();
    let object_id3 = ObjectID::random();
    let coin_obj1 = Object::with_id_owner_gas_for_testing(object_id1, sender, 5000000);
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
        5000000,
    )
    .await;
    let recipient_amount_map: HashMap<_, u64> =
        HashMap::from([(recipient1, 500), (recipient2, 1500)]);
    let effects = res.txn_result.unwrap().into_data();
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // make sure each recipient receives the specified amount
    assert_eq!(effects.created().len(), 2);
    let created_obj_id1 = effects.created()[0].0 .0;
    let created_obj_id2 = effects.created()[1].0 .0;
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
    let addr1 = effects.created()[0].1.get_owner_address()?;
    let addr2 = effects.created()[1].1.get_owner_address()?;
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
    assert_eq!(effects.mutated()[0].0 .0, object_id1);
    assert_eq!(effects.mutated()[0].1, sender);
    let gas_used = effects.gas_cost_summary().net_gas_usage() as u64;
    let gas_object = res.authority_state.get_object(&object_id1).await?.unwrap();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        5002000 - 500 - 1500 - gas_used,
    );

    // make sure the second and third input coins are deleted
    let deleted_ids: Vec<ObjectID> = effects.deleted().iter().map(|d| d.0).collect();
    assert!(deleted_ids.contains(&object_id2));
    assert!(deleted_ids.contains(&object_id3));
    Ok(())
}

#[tokio::test]
async fn test_pay_all_sui_failure_insufficient_gas_one_input_coin() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let coin1 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1800);
    let recipient = dbg_addr(2);

    let res = execute_pay_all_sui(vec![&coin1], recipient, sender, sender_key, 2000).await;

    assert_eq!(
        UserInputError::try_from(res.txn_result.unwrap_err()).unwrap(),
        UserInputError::GasBalanceTooLow {
            gas_balance: 1800,
            needed_gas_amount: 2000,
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

    assert_eq!(
        UserInputError::try_from(res.txn_result.unwrap_err()).unwrap(),
        UserInputError::GasBalanceTooLow {
            gas_balance: 2000,
            needed_gas_amount: 2500,
        }
    );
}

#[tokio::test]
async fn test_pay_all_sui_success_one_input_coin() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id = ObjectID::random();
    let coin_obj = Object::with_id_owner_gas_for_testing(object_id, sender, 3000000);
    let recipient = dbg_addr(2);
    let res = execute_pay_all_sui(vec![&coin_obj], recipient, sender, sender_key, 2000000).await;

    let effects = res.txn_result.unwrap().into_data();
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // make sure the first object now belongs to the recipient,
    // the value is equal to all residual values after gas payment.
    let obj_ref = &effects.mutated()[0].0;
    assert_eq!(obj_ref.0, object_id);
    assert_eq!(effects.mutated()[0].1, recipient);

    let gas_used = effects.gas_cost_summary().gas_used();
    let gas_object = res.authority_state.get_object(&object_id).await?.unwrap();
    assert_eq!(GasCoin::try_from(&gas_object)?.value(), 3000000 - gas_used,);
    Ok(())
}

#[tokio::test]
async fn test_pay_all_sui_success_multiple_input_coins() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id1 = ObjectID::random();
    let coin_obj1 = Object::with_id_owner_gas_for_testing(object_id1, sender, 3000000);
    let coin_obj2 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let coin_obj3 = Object::with_id_owner_gas_for_testing(ObjectID::random(), sender, 1000);
    let recipient = dbg_addr(2);
    let res = execute_pay_all_sui(
        vec![&coin_obj1, &coin_obj2, &coin_obj3],
        recipient,
        sender,
        sender_key,
        3000000,
    )
    .await;

    let effects = res.txn_result.unwrap().into_data();
    assert_eq!(*effects.status(), ExecutionStatus::Success);

    // make sure the first object now belongs to the recipient,
    // the value is equal to all residual values after gas payment.
    let obj_ref = &effects.mutated()[0].0;
    assert_eq!(obj_ref.0, object_id1);
    assert_eq!(effects.mutated()[0].1, recipient);

    let gas_used = effects.gas_cost_summary().gas_used();
    let gas_object = res.authority_state.get_object(&object_id1).await?.unwrap();
    assert_eq!(GasCoin::try_from(&gas_object)?.value(), 3002000 - gas_used,);
    Ok(())
}

struct PaySuiTransactionBlockExecutionResult {
    pub authority_state: Arc<AuthorityState>,
    pub txn_result: Result<SignedTransactionEffects, SuiError>,
}

async fn execute_pay_sui(
    input_coin_objects: Vec<Object>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_budget: u64,
) -> PaySuiTransactionBlockExecutionResult {
    let authority_state = TestAuthorityBuilder::new().build().await;

    let input_coin_refs: Vec<ObjectRef> = input_coin_objects
        .iter()
        .map(|coin_obj| coin_obj.compute_object_reference())
        .collect();
    let handles: Vec<_> = input_coin_objects
        .into_iter()
        .map(|obj| authority_state.insert_genesis_object(obj))
        .collect();
    join_all(handles).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.pay_sui(recipients, amounts).unwrap();
    let pt = builder.finish();
    let data = TransactionData::new_programmable(sender, input_coin_refs, pt, gas_budget, 1);
    let tx = to_sender_signed_transaction(data, &sender_key);
    let txn_result = send_and_confirm_transaction(&authority_state, tx)
        .await
        .map(|(_, effects)| effects);

    PaySuiTransactionBlockExecutionResult {
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
) -> PaySuiTransactionBlockExecutionResult {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
        // TODO: fix numbers in tests to not depend on rgp being 1
        .with_reference_gas_price(1)
        .with_objects(
            input_coin_objects
                .clone()
                .into_iter()
                .map(ToOwned::to_owned),
        )
        .build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0].protocol_key_pair();

    let authority_state = init_state_with_committee(&genesis, keypair).await;

    let mut input_coins = Vec::new();
    for coin in input_coin_objects {
        let id = coin.id();
        let object_ref = genesis
            .objects()
            .iter()
            .find(|o| o.id() == id)
            .unwrap()
            .compute_object_reference();
        input_coins.push(object_ref);
    }

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.pay_all_sui(recipient);
    let pt = builder.finish();
    let data = TransactionData::new_programmable(sender, input_coins, pt, gas_budget, 1);
    let tx = to_sender_signed_transaction(data, &sender_key);
    let txn_result = send_and_confirm_transaction(&authority_state, tx)
        .await
        .map(|(_, effects)| effects);
    PaySuiTransactionBlockExecutionResult {
        authority_state,
        txn_result,
    }
}
