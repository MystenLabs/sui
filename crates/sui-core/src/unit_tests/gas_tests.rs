// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;

use super::authority_tests::{init_state_with_ids, send_and_confirm_transaction};
use super::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::authority_tests::init_state;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use sui_adapter::genesis;
use sui_types::crypto::AccountKeyPair;
use sui_types::gas_coin::GasCoin;
use sui_types::object::GAS_VALUE_FOR_TESTING;
use sui_types::{
    base_types::dbg_addr,
    crypto::get_key_pair,
    gas::{SuiGasStatus, MAX_GAS_BUDGET, MIN_GAS_BUDGET},
    messages::Transaction,
};

#[tokio::test]
async fn test_tx_less_than_minimum_gas_budget() {
    // This test creates a transaction that sets a gas_budget less than the minimum
    // transaction requirement. It's expected to fail early during transaction
    // handling phase.
    let budget = *MIN_GAS_BUDGET - 1;
    let result = execute_transfer(*MAX_GAS_BUDGET, budget, false).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas budget is {}, smaller than minimum requirement {}",
                budget, *MIN_GAS_BUDGET
            )
        }
    );
}

#[tokio::test]
async fn test_tx_more_than_maximum_gas_budget() {
    // This test creates a transaction that sets a gas_budget more than the maximum
    // budget (which could lead to overflow). It's expected to fail early during transaction
    // handling phase.
    let budget = *MAX_GAS_BUDGET + 1;
    let result = execute_transfer(*MAX_GAS_BUDGET, budget, false).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!("Gas budget set too high; maximum is {}", *MAX_GAS_BUDGET)
        }
    );
}

#[tokio::test]
async fn test_tx_gas_balance_less_than_budget() {
    // This test creates a transaction that uses a gas object whose balance
    // is not even enough to pay for the gas budget. This should fail early
    // during handle transaction phase.
    let gas_balance = *MIN_GAS_BUDGET - 1;
    let budget = *MIN_GAS_BUDGET;
    let gas_price = 1;
    let result = execute_transfer_with_price(gas_balance, budget, gas_price, false).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                gas_balance,
                gas_price * budget,
                gas_price
            )
        }
    );
}

#[tokio::test]
async fn test_native_transfer_sufficient_gas() -> SuiResult {
    // This test does a native transfer with sufficient gas budget and balance.
    // It's expected to succeed. We check that gas was charged properly.
    let result = execute_transfer(*MAX_GAS_BUDGET, *MAX_GAS_BUDGET, true).await;
    let effects = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    let gas_cost = effects.gas_used;
    assert!(gas_cost.computation_cost > *MIN_GAS_BUDGET);
    assert!(gas_cost.storage_cost > 0);
    // Removing genesis object does not have rebate.
    assert_eq!(gas_cost.storage_rebate, 0);

    let object = result
        .authority_state
        .get_object(&result.object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = result
        .authority_state
        .get_object(&result.gas_object_id)
        .await?
        .unwrap();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        *MAX_GAS_BUDGET - gas_cost.gas_used()
    );

    // Mimic the process of gas charging, to check that we are charging
    // exactly what we should be charging.
    let mut gas_status = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET, 1, 1);
    gas_status.charge_min_tx_gas()?;
    let obj_size = object.object_size_for_gas_metering();
    let gas_size = gas_object.object_size_for_gas_metering();

    gas_status.charge_storage_read(obj_size + gas_size)?;
    gas_status.charge_storage_mutation(obj_size, obj_size, 0)?;
    gas_status.charge_storage_mutation(gas_size, gas_size, 0)?;
    assert_eq!(&gas_cost, &gas_status.summary(true));
    Ok(())
}

#[tokio::test]
async fn test_native_transfer_gas_price_is_used() {
    let gas_price_1 = 1;
    let gas_price_2 = gas_price_1 * 2;
    let result =
        execute_transfer_with_price(*MAX_GAS_BUDGET, *MAX_GAS_BUDGET, gas_price_1, true).await;
    let effects = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    let gas_summary_1 = effects.gas_cost_summary();

    let result =
        execute_transfer_with_price(*MAX_GAS_BUDGET, *MAX_GAS_BUDGET / 2, gas_price_2, true).await;
    let effects = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    let gas_summary_2 = effects.gas_cost_summary();

    assert_eq!(
        gas_summary_1.computation_cost * 2,
        gas_summary_2.computation_cost
    );

    // test overflow with insufficient gas
    let gas_balance = *MAX_GAS_BUDGET;
    let gas_budget = *MAX_GAS_BUDGET;
    let gas_price = u64::MAX;
    let result = execute_transfer_with_price(gas_balance, gas_budget, gas_price, true).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {} with gas price of {}",
                gas_balance,
                (gas_budget as u128) * (gas_price as u128),
                gas_price
            )
        }
    );
}

#[tokio::test]
async fn test_transfer_sui_insufficient_gas() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, 50);
    let gas_object_ref = gas_object.compute_object_reference();
    let authority_state = init_state().await;
    authority_state.insert_genesis_object(gas_object).await;

    let kind = TransactionKind::Single(SingleTransactionKind::TransferSui(TransferSui {
        recipient,
        amount: None,
    }));
    let data = TransactionData::new_with_gas_price(kind, sender, gas_object_ref, 50, 1);
    let tx = Transaction::from_data(data, &sender_key);

    let effects = send_and_confirm_transaction(&authority_state, tx)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    // We expect this to fail due to insufficient gas.
    assert_eq!(
        effects.status,
        ExecutionStatus::new_failure(ExecutionFailureStatus::InsufficientGas)
    );
    // Ensure that the owner of the object did not change if the transfer failed.
    assert_eq!(effects.mutated[0].1, sender);
}

#[tokio::test]
async fn test_native_transfer_insufficient_gas_reading_objects() {
    // This test creates a transfer transaction with a gas budget, that's more than
    // the minimum budget requirement, but not enough to even read the objects from db.
    // This will lead to failure in lock check step during handle transaction phase.
    let balance = *MIN_GAS_BUDGET + 1;
    let result = execute_transfer(balance, balance, true).await;
    // The transaction should still execute to effects, but with execution status as failure.
    let effects = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    assert_eq!(
        effects.status.unwrap_err(),
        ExecutionFailureStatus::InsufficientGas
    );
}

#[tokio::test]
async fn test_native_transfer_insufficient_gas_execution() {
    // This test creates a transfer transaction with a gas budget that's insufficient
    // to finalize the transfer object mutation effects. It will fail during
    // execution phase, and hence gas object will still be mutated and all budget
    // will be charged.
    let result = execute_transfer(*MAX_GAS_BUDGET, *MAX_GAS_BUDGET, true).await;
    let total_gas = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .gas_used
        .gas_used();
    let budget = total_gas - 1;
    let result = execute_transfer(budget, budget, true).await;
    let effects = result
        .response
        .unwrap()
        .signed_effects
        .unwrap()
        .effects()
        .clone();
    // We won't drain the entire budget because we don't charge for storage if tx failed.
    assert!(effects.gas_used.gas_used() < budget);
    let gas_object = result
        .authority_state
        .get_object(&result.gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_coin = GasCoin::try_from(&gas_object).unwrap();
    assert_eq!(gas_coin.value(), budget - effects.gas_used.gas_used());
    // After a failed transfer, the version should have been incremented,
    // but the owner of the object should remain the same, unchanged.
    let ((_, version, _), owner) = effects.mutated_excluding_gas().next().unwrap();
    assert_eq!(version, &gas_object.version());
    assert_eq!(owner, &gas_object.owner);

    assert_eq!(
        effects.status.unwrap_err(),
        ExecutionFailureStatus::InsufficientGas,
    );
}

#[tokio::test]
async fn test_publish_gas() -> anyhow::Result<()> {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    // The successful case.
    let response = build_and_try_publish_test_package(
        &authority_state,
        &sender,
        &sender_key,
        &gas_object_id,
        "object_wrapping",
        GAS_VALUE_FOR_TESTING,
    )
    .await;
    let effects = response.signed_effects.unwrap().effects().clone();
    let gas_cost = effects.gas_used;
    assert!(gas_cost.storage_cost > 0);

    let ((package_id, _, _), _) = effects.created[0];
    let package = authority_state.get_object(&package_id).await?.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = GAS_VALUE_FOR_TESTING - gas_cost.gas_used();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );
    // genesis objects are read during transaction since they are direct dependencies.
    let genesis_objects = genesis::clone_genesis_packages();
    // We need the original package bytes in order to reproduce the publish computation cost.
    let publish_bytes = match response
        .certified_transaction
        .as_ref()
        .unwrap()
        .data()
        .data
        .kind
        .single_transactions()
        .next()
        .unwrap()
    {
        SingleTransactionKind::Publish(p) => &p.modules,
        _ => unreachable!(),
    };

    // Mimic the gas charge behavior and cross check the result with above.
    let mut gas_status = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET, 1, 1);
    gas_status.charge_min_tx_gas()?;
    gas_status.charge_storage_read(
        genesis_objects
            .iter()
            .map(|o| o.object_size_for_gas_metering())
            .sum(),
    )?;
    gas_status.charge_storage_read(gas_object.object_size_for_gas_metering())?;
    gas_status.charge_publish_package(publish_bytes.iter().map(|v| v.len()).sum())?;
    gas_status.charge_storage_mutation(0, package.object_size_for_gas_metering(), 0)?;
    // Remember the gas used so far. We will use this to create another failure case latter.
    let gas_used_after_package_creation = gas_status.summary(true).gas_used();
    gas_status.charge_storage_mutation(
        gas_object.object_size_for_gas_metering(),
        gas_object.object_size_for_gas_metering(),
        0,
    )?;
    assert_eq!(&gas_cost, &gas_status.summary(true));

    // Create a transaction with budget DELTA less than the gas cost required.
    let total_gas_used = gas_cost.gas_used();
    let computation_cost = gas_cost.computation_cost;
    const DELTA: u64 = 1;
    let budget = total_gas_used - DELTA;
    // Run the transaction again with 1 less than the required budget.
    let response = build_and_try_publish_test_package(
        &authority_state,
        &sender,
        &sender_key,
        &gas_object_id,
        "object_wrapping",
        budget,
    )
    .await;
    let effects = response.signed_effects.unwrap().effects().clone();
    let gas_cost = effects.gas_used;
    let err = effects.status.unwrap_err();

    assert_eq!(err, ExecutionFailureStatus::InsufficientGas);

    // Make sure that we are not charging storage cost at failure.
    assert_eq!(gas_cost.storage_cost, 0);
    // Upon failure, we should only be charging the expected computation cost.
    assert_eq!(gas_cost.gas_used(), computation_cost);

    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = expected_gas_balance - gas_cost.gas_used();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );

    // Create a transaction with gas_budget that's 1 less than the amount needed to
    // finish charging for storage. This will lead to out of gas failure while trying
    // to deduct gas for storage.
    let budget = gas_used_after_package_creation - 1;
    let response = build_and_try_publish_test_package(
        &authority_state,
        &sender,
        &sender_key,
        &gas_object_id,
        "object_wrapping",
        budget,
    )
    .await;
    let effects = response.signed_effects.unwrap().effects().clone();
    let gas_cost = effects.gas_used;
    let err = effects.status.unwrap_err();
    assert_eq!(err, ExecutionFailureStatus::InsufficientGas);
    assert_eq!(gas_cost.storage_cost, 0);
    assert_eq!(gas_cost.storage_rebate, 0);
    Ok(())
}

#[tokio::test]
async fn test_move_call_gas() -> SuiResult {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;
    let package_object_ref = authority_state.get_framework_object_ref().await?;
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();

    let module = ident_str!("object_basics").to_owned();
    let function = ident_str!("create").to_owned();
    let args = vec![
        CallArg::Pure(16u64.to_le_bytes().to_vec()),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
    ];
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module.clone(),
        function.clone(),
        Vec::new(),
        gas_object.compute_object_reference(),
        args.clone(),
        GAS_VALUE_FOR_TESTING,
    );
    let transaction = Transaction::from_data(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects().clone();
    let created_object_ref = effects.created[0].0;
    assert!(effects.status.is_ok());
    let gas_cost = effects.gas_used;
    assert!(gas_cost.storage_cost > 0);
    assert_eq!(gas_cost.storage_rebate, 0);
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = GAS_VALUE_FOR_TESTING - gas_cost.gas_used();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );

    // Mimic the gas charge behavior and cross check the result with above. Do not include
    // computation cost calculation as it would require hard-coding a constant representing VM
    // execution cost which is quite fragile.
    let mut gas_status = SuiGasStatus::new_with_budget(GAS_VALUE_FOR_TESTING, 1, 1);
    gas_status.charge_min_tx_gas()?;
    let package_object = authority_state
        .get_object(&package_object_ref.0)
        .await?
        .unwrap();
    gas_status.charge_storage_read(
        package_object.object_size_for_gas_metering() + gas_object.object_size_for_gas_metering(),
    )?;
    let gas_used_before_vm_exec = gas_status.summary(true).gas_used();
    let created_object = authority_state
        .get_object(&effects.created[0].0 .0)
        .await?
        .unwrap();
    gas_status.charge_storage_mutation(0, created_object.object_size_for_gas_metering(), 0)?;
    gas_status.charge_storage_mutation(
        gas_object.object_size_for_gas_metering(),
        gas_object.object_size_for_gas_metering(),
        0,
    )?;

    let new_cost = gas_status.summary(true);
    assert_eq!(gas_cost.storage_cost, new_cost.storage_cost);
    // This is the total amount of storage cost paid. We will use this
    // to check if we get back the same amount of rebate latter.
    let prev_storage_cost = gas_cost.storage_cost;

    // Execute object deletion, and make sure we have storage rebate.
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module.clone(),
        ident_str!("delete").to_owned(),
        vec![],
        gas_object.compute_object_reference(),
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
            created_object_ref,
        ))],
        expected_gas_balance,
    );
    let transaction = Transaction::from_data(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects().clone();
    assert!(effects.status.is_ok());
    let gas_cost = effects.gas_used;
    // storage_cost should be less than rebate because for object deletion, we only
    // rebate without charging.
    assert!(gas_cost.storage_cost > 0 && gas_cost.storage_cost < gas_cost.storage_rebate);
    // Check that we have storage rebate that's the same as previous cost.
    assert_eq!(gas_cost.storage_rebate, prev_storage_cost);
    let expected_gas_balance = expected_gas_balance - gas_cost.gas_used() + gas_cost.storage_rebate;

    // Create a transaction with gas budget that should run out during Move VM execution.
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let budget = gas_used_before_vm_exec + 1;
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module,
        function,
        Vec::new(),
        gas_object.compute_object_reference(),
        args,
        budget,
    );
    let transaction = Transaction::from_data(data, &sender_key);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects().clone();
    let gas_cost = effects.gas_used;
    let err = effects.status.unwrap_err();
    // We will run out of gas during VM execution.
    assert!(matches!(err, ExecutionFailureStatus::InsufficientGas));
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = expected_gas_balance - gas_cost.gas_used() + gas_cost.storage_rebate;
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );
    Ok(())
}

#[tokio::test]
async fn test_storage_gas_unit_price() -> SuiResult {
    let mut gas_status1 = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET, 1, 1);
    gas_status1.charge_storage_mutation(100, 200, 5)?;
    let gas_cost1 = gas_status1.summary(true);
    let mut gas_status2 = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET, 1, 3);
    gas_status2.charge_storage_mutation(100, 200, 5)?;
    let gas_cost2 = gas_status2.summary(true);
    // Computation unit price is the same, hence computation cost should be the same.
    assert_eq!(gas_cost1.computation_cost, gas_cost2.computation_cost);
    // Storage unit prices is 3X, so will be the storage cost.
    assert_eq!(gas_cost1.storage_cost * 3, gas_cost2.storage_cost);
    // Storage rebate should not be affected by the price.
    assert_eq!(gas_cost1.storage_rebate, gas_cost2.storage_rebate);
    Ok(())
}

struct TransferResult {
    pub authority_state: AuthorityState,
    pub object_id: ObjectID,
    pub gas_object_id: ObjectID,
    pub response: SuiResult<TransactionInfoResponse>,
}

async fn execute_transfer(gas_balance: u64, gas_budget: u64, run_confirm: bool) -> TransferResult {
    execute_transfer_with_price(gas_balance, gas_budget, 1, run_confirm).await
}

async fn execute_transfer_with_price(
    gas_balance: u64,
    gas_budget: u64,
    gas_price: u64,
    run_confirm: bool,
) -> TransferResult {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, gas_balance);
    let gas_object_ref = gas_object.compute_object_reference();
    authority_state.insert_genesis_object(gas_object).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    let kind = TransactionKind::Single(SingleTransactionKind::TransferObject(TransferObject {
        recipient,
        object_ref: object.compute_object_reference(),
    }));
    let data =
        TransactionData::new_with_gas_price(kind, sender, gas_object_ref, gas_budget, gas_price);
    let tx = Transaction::from_data(data, &sender_key);

    let response = if run_confirm {
        send_and_confirm_transaction(&authority_state, tx).await
    } else {
        authority_state.handle_transaction(tx).await
    };
    TransferResult {
        authority_state,
        object_id,
        gas_object_id,
        response,
    }
}
