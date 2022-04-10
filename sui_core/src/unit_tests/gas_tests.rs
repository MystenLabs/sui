// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_tests::get_genesis_package_by_module;

use super::*;

use super::authority_tests::{init_state_with_ids, send_and_confirm_transaction};
use super::move_integration_tests::build_and_try_publish_test_package;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use sui_adapter::genesis;
use sui_types::gas_coin::GasCoin;
use sui_types::object::GAS_VALUE_FOR_TESTING;
use sui_types::{
    base_types::dbg_addr,
    crypto::{get_key_pair, Signature},
    gas::{MAX_GAS_BUDGET, MIN_GAS_BUDGET},
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
    let result = execute_transfer(gas_balance, budget, false).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: format!(
                "Gas balance is {}, not enough to pay {}",
                gas_balance, budget
            )
        }
    );
}

#[tokio::test]
async fn test_native_transfer_sufficient_gas() -> SuiResult {
    // This test does a native transfer with sufficient gas budget and balance.
    // It's expected to succeed. We check that gas was charged properly.
    let result = execute_transfer(*MAX_GAS_BUDGET, *MAX_GAS_BUDGET, true).await;
    let effects = result.response.unwrap().signed_effects.unwrap().effects;
    let gas_cost = effects.status.gas_cost_summary();
    assert!(gas_cost.computation_cost > *MIN_GAS_BUDGET);
    assert_eq!(gas_cost.storage_cost, 0);
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
    let mut gas_status = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET);
    gas_status.charge_min_tx_gas()?;

    // Both the object to be transferred and the gas object will be read
    // from the store. Hence we need to charge for 2 reads.
    gas_status.charge_storage_read(
        object.object_size_for_gas_metering() + gas_object.object_size_for_gas_metering(),
    )?;
    gas_status.charge_storage_mutation(
        object.object_size_for_gas_metering(),
        object.object_size_for_gas_metering(),
    )?;
    gas_status.charge_storage_mutation(
        gas_object.object_size_for_gas_metering(),
        gas_object.object_size_for_gas_metering(),
    )?;
    assert_eq!(gas_cost, &gas_status.summary(true));
    Ok(())
}

#[tokio::test]
async fn test_native_transfer_insufficient_gas_reading_objects() {
    // This test creates a transfer transaction with a gas budget, that's more than
    // the minimum budget requirement, but not enough to even read the objects from db.
    // This will lead to failure in lock check step during handle transaction phase.
    let balance = *MIN_GAS_BUDGET + 1;
    let result = execute_transfer(balance, balance, false).await;
    let err = result.response.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: "Ran out of gas while deducting computation cost".to_owned()
        }
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
        .effects
        .status
        .gas_cost_summary()
        .gas_used();
    let budget = total_gas - 1;
    let result = execute_transfer(budget, budget, true).await;
    let effects = result.response.unwrap().signed_effects.unwrap().effects;
    // The gas balance should be drained.
    assert_eq!(effects.status.gas_cost_summary().gas_used(), budget);
    let gas_object = result
        .authority_state
        .get_object(&result.gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_coin = GasCoin::try_from(&gas_object).unwrap();
    assert_eq!(gas_coin.value(), 0);
    assert_eq!(
        effects.status.unwrap_err().1,
        SuiError::InsufficientGas {
            error: "Ran out of gas while deducting computation cost".to_owned()
        }
    );
}

#[tokio::test]
async fn test_publish_gas() -> SuiResult {
    let (sender, sender_key) = get_key_pair();
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
    let effects = response.signed_effects.unwrap().effects;
    let gas_cost = effects.status.gas_cost_summary();
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
        .transaction
        .single_transactions()
        .next()
        .unwrap()
    {
        SingleTransactionKind::Publish(p) => &p.modules,
        _ => unreachable!(),
    };

    // Mimic the gas charge behavior and cross check the result with above.
    let mut gas_status = SuiGasStatus::new_with_budget(*MAX_GAS_BUDGET);
    gas_status.charge_min_tx_gas()?;
    gas_status.charge_storage_read(
        genesis_objects
            .iter()
            .map(|o| o.object_size_for_gas_metering())
            .sum(),
    )?;
    gas_status.charge_storage_read(gas_object.object_size_for_gas_metering())?;
    gas_status.charge_publish_package(publish_bytes.iter().map(|v| v.len()).sum())?;
    gas_status.charge_storage_mutation(0, package.object_size_for_gas_metering())?;
    // Remember the gas used so far. We will use this to create another failure case latter.
    let gas_used_after_package_creation = gas_status.summary(true).gas_used();
    gas_status.charge_storage_mutation(
        gas_object.object_size_for_gas_metering(),
        gas_object.object_size_for_gas_metering(),
    )?;
    assert_eq!(gas_cost, &gas_status.summary(true));

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
    let effects = response.signed_effects.unwrap().effects;
    let (gas_cost, err) = effects.status.unwrap_err();

    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: "Ran out of gas while deducting computation cost".to_owned()
        }
    );

    // Make sure that we are not charging storage cost at failure.
    assert_eq!(gas_cost.storage_cost, 0);
    // Upon failure, we should only be charging the expected computation cost.
    // Since we failed when trying to charge the last piece of computation cost,
    // the total cost will be DELTA less since it's not enough.
    assert_eq!(gas_cost.gas_used(), computation_cost - DELTA);

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
    let effects = response.signed_effects.unwrap().effects;
    let (gas_cost, err) = effects.status.unwrap_err();
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: "Ran out of gas while deducting storage cost".to_owned()
        }
    );
    assert_eq!(gas_cost.storage_cost, 0);
    assert_eq!(gas_cost.storage_rebate, 0);
    Ok(())
}

#[tokio::test]
async fn test_move_call_gas() -> SuiResult {
    let (sender, sender_key) = get_key_pair();
    let gas_object_id = ObjectID::random();
    // find the function Object::create and call it to create a new object
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref =
        get_genesis_package_by_module(&genesis_package_objects, "ObjectBasics");

    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();

    let module = ident_str!("ObjectBasics").to_owned();
    let function = ident_str!("create").to_owned();
    let pure_args = vec![
        16u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(sender)).unwrap(),
    ];
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module.clone(),
        function.clone(),
        Vec::new(),
        gas_object.compute_object_reference(),
        Vec::new(),
        vec![],
        pure_args.clone(),
        GAS_VALUE_FOR_TESTING,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects;
    let created_object_ref = effects.created[0].0;
    assert!(effects.status.is_ok());
    let gas_cost = effects.status.gas_cost_summary();
    assert!(gas_cost.storage_cost > 0);
    assert_eq!(gas_cost.storage_rebate, 0);
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = GAS_VALUE_FOR_TESTING - gas_cost.gas_used();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );

    // Mimic the gas charge behavior and cross check the result with above.
    let mut gas_status = SuiGasStatus::new_with_budget(GAS_VALUE_FOR_TESTING);
    gas_status.charge_min_tx_gas()?;
    let package_object = authority_state
        .get_object(&package_object_ref.0)
        .await?
        .unwrap();
    gas_status.charge_storage_read(
        package_object.object_size_for_gas_metering() + gas_object.object_size_for_gas_metering(),
    )?;
    let gas_used_before_vm_exec = gas_status.summary(true).gas_used();
    // The gas cost to execute the function in Move VM.
    // Hard code it here since it's difficult to mock that in test.
    const MOVE_VM_EXEC_COST: u64 = 17006;
    gas_status.charge_vm_exec_test_only(MOVE_VM_EXEC_COST)?;
    let created_object = authority_state
        .get_object(&effects.created[0].0 .0)
        .await?
        .unwrap();
    gas_status.charge_storage_mutation(0, created_object.object_size_for_gas_metering())?;
    gas_status.charge_storage_mutation(
        gas_object.object_size_for_gas_metering(),
        gas_object.object_size_for_gas_metering(),
    )?;

    let new_cost = gas_status.summary(true);
    assert_eq!(gas_cost.computation_cost, new_cost.computation_cost,);
    assert_eq!(gas_cost.storage_cost, new_cost.storage_cost);

    // Create a transaction with gas budget that should run out during Move VM execution.
    let budget = gas_used_before_vm_exec + 1;
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module.clone(),
        function,
        Vec::new(),
        gas_object.compute_object_reference(),
        Vec::new(),
        vec![],
        pure_args,
        budget,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects;
    let (gas_cost, err) = effects.status.unwrap_err();
    // This is to show that even though we ran out of gas during Move VM execution,
    // we will still try to charge for gas object mutation, which will lead to
    // the error below.
    assert_eq!(
        err,
        SuiError::InsufficientGas {
            error: "Ran out of gas while deducting computation cost".to_owned()
        }
    );
    let gas_object = authority_state.get_object(&gas_object_id).await?.unwrap();
    let expected_gas_balance = expected_gas_balance - gas_cost.gas_used();
    assert_eq!(
        GasCoin::try_from(&gas_object)?.value(),
        expected_gas_balance,
    );

    // Execute object deletion, and make sure we have storage rebate.
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        module,
        ident_str!("delete").to_owned(),
        vec![],
        gas_object.compute_object_reference(),
        vec![created_object_ref],
        vec![],
        vec![],
        expected_gas_balance,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let response = send_and_confirm_transaction(&authority_state, transaction).await?;
    let effects = response.signed_effects.unwrap().effects;
    assert!(effects.status.is_ok());
    let gas_cost = effects.status.gas_cost_summary();
    assert_eq!(gas_cost.storage_cost, 0);
    // Check that we have storage rebate after deletion.
    assert!(gas_cost.storage_rebate > 0);
    Ok(())
}

struct TransferResult {
    pub authority_state: AuthorityState,
    pub object_id: ObjectID,
    pub gas_object_id: ObjectID,
    pub response: SuiResult<TransactionInfoResponse>,
}

async fn execute_transfer(gas_balance: u64, gas_budget: u64, run_confirm: bool) -> TransferResult {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_coin_object_for_testing(
        gas_object_id,
        SequenceNumber::new(),
        sender,
        gas_balance,
    );
    let gas_object_ref = gas_object.compute_object_reference();
    authority_state.insert_genesis_object(gas_object).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    let data = TransactionData::new_transfer(
        recipient,
        object.compute_object_reference(),
        sender,
        gas_object_ref,
        gas_budget,
    );
    let signature = Signature::new(&data, &sender_key);
    let tx = Transaction::new(data, signature);

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
