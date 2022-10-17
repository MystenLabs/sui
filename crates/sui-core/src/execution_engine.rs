// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::Identifier;
use std::{collections::BTreeSet, sync::Arc};
use sui_types::id::UID;
use sui_types::storage::{ChildObjectResolver, DeleteKind, ParentSync, WriteKind};
#[cfg(test)]
use sui_types::temporary_store;
use sui_types::temporary_store::InnerTemporaryStore;
use sui_types::SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION;

use crate::authority::TemporaryStore;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use sui_adapter::adapter;
use sui_types::coin::{transfer_coin, update_input_coins, Coin};
use sui_types::committee::EpochId;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;

#[cfg(test)]
use sui_types::messages::ExecutionFailureStatus;
#[cfg(test)]
use sui_types::messages::InputObjects;
use sui_types::messages::{ObjectArg, Pay, PayAllSui, PaySui};
use sui_types::object::{Data, MoveObject, Owner, OBJECT_START_VERSION};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, TxContext},
    event::{Event, TransferType},
    gas::{self, SuiGasStatus},
    messages::{
        CallArg, ChangeEpoch, ExecutionStatus, MoveCall, MoveModulePublish, SingleTransactionKind,
        TransactionData, TransactionEffects, TransferObject, TransferSui,
    },
    object::Object,
    storage::BackingPackageStore,
    sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use tracing::{debug, instrument, trace};

#[cfg(test)]
#[path = "unit_tests/pay_sui_tests.rs"]
mod pay_sui_tests;

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    shared_object_refs: Vec<ObjectRef>,
    mut temporary_store: TemporaryStore<S>,
    transaction_data: TransactionData,
    transaction_digest: TransactionDigest,
    mut transaction_dependencies: BTreeSet<TransactionDigest>,
    move_vm: &Arc<MoveVM>,
    native_functions: &NativeFunctionTable,
    gas_status: SuiGasStatus,
    epoch: EpochId,
) -> (
    InnerTemporaryStore,
    TransactionEffects,
    Option<ExecutionError>,
) {
    let mut tx_ctx = TxContext::new(&transaction_data.signer(), &transaction_digest, epoch);

    let gas_object_ref = *transaction_data.gas_payment_object_ref();
    let (gas_cost_summary, execution_result) = execute_transaction(
        &mut temporary_store,
        transaction_data,
        gas_object_ref.0,
        &mut tx_ctx,
        move_vm,
        native_functions,
        gas_status,
    );

    let (status, execution_error) = match execution_result {
        Ok(()) => (ExecutionStatus::Success, None),
        Err(error) => (
            ExecutionStatus::new_failure(error.to_execution_status()),
            Some(error),
        ),
    };
    debug!(
        computation_gas_cost = gas_cost_summary.computation_cost,
        storage_gas_cost = gas_cost_summary.storage_cost,
        storage_gas_rebate = gas_cost_summary.storage_rebate,
        "Finished execution of transaction with status {:?}",
        status
    );

    // Remove from dependencies the generic hash
    transaction_dependencies.remove(&TransactionDigest::genesis());

    let (inner, effects) = temporary_store.to_effects(
        shared_object_refs,
        &transaction_digest,
        transaction_dependencies.into_iter().collect(),
        gas_cost_summary,
        status,
        gas_object_ref,
    );
    (inner, effects, execution_error)
}

fn charge_gas_for_object_read<S>(
    temporary_store: &TemporaryStore<S>,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    // Charge gas for reading all objects from the DB.
    // TODO: Some of the objects may be duplicate (for batch tx). We could save gas by
    // fetching only unique objects.
    let total_size = temporary_store
        .objects()
        .values()
        .map(|obj| obj.object_size_for_gas_metering())
        .sum();
    gas_status.charge_storage_read(total_size)
}

#[instrument(name = "tx_execute", level = "debug", skip_all)]
fn execute_transaction<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    temporary_store: &mut TemporaryStore<S>,
    transaction_data: TransactionData,
    gas_object_id: ObjectID,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    native_functions: &NativeFunctionTable,
    mut gas_status: SuiGasStatus,
) -> (GasCostSummary, Result<(), ExecutionError>) {
    // We must charge object read gas inside here during transaction execution, because if this fails
    // we must still ensure an effect is committed and all objects versions incremented.
    let mut result = charge_gas_for_object_read(temporary_store, &mut gas_status);
    if result.is_ok() {
        // TODO: Since we require all mutable objects to not show up more than
        // once across single tx, we should be able to run them in parallel.
        for single_tx in transaction_data.kind.into_single_transactions() {
            result = match single_tx {
                SingleTransactionKind::TransferObject(TransferObject {
                    recipient,
                    object_ref,
                }) => {
                    // unwrap is safe because we built the object map from the transactions
                    let object = temporary_store
                        .objects()
                        .get(&object_ref.0)
                        .unwrap()
                        .clone();
                    transfer_object(temporary_store, object, tx_ctx.sender(), recipient)
                }
                SingleTransactionKind::TransferSui(TransferSui { recipient, amount }) => {
                    let gas_object = temporary_store
                        .objects()
                        .get(&gas_object_id)
                        .expect("We constructed the object map so it should always have the gas object id")
                        .clone();
                    transfer_sui(temporary_store, gas_object, recipient, amount, tx_ctx)
                }
                SingleTransactionKind::PaySui(PaySui {
                    coins,
                    recipients,
                    amounts,
                }) => {
                    let mut coin_objects: Vec<Object> =  // unwrap is is safe because we built the object map from the transaction
                    coins.iter().map(|c|
                    temporary_store
                        .objects()
                        .get(&c.0)
                        .unwrap()
                        .clone()
                    ).collect();
                    pay_sui(
                        temporary_store,
                        &mut coin_objects,
                        recipients,
                        amounts,
                        tx_ctx,
                    )
                }
                SingleTransactionKind::PayAllSui(PayAllSui { coins, recipient }) => {
                    let mut coin_objects: Vec<Object> =  // unwrap is is safe because we built the object map from the transaction
                    coins.iter().map(|c|
                    temporary_store
                        .objects()
                        .get(&c.0)
                        .unwrap()
                        .clone()
                    ).collect();
                    pay_all_sui(temporary_store, &mut coin_objects, recipient)
                }
                SingleTransactionKind::Call(MoveCall {
                    package,
                    module,
                    function,
                    type_arguments,
                    arguments,
                }) => {
                    // Charge gas for this VM execution
                    if let Err(e) = gas_status.charge_vm_gas() {
                        result = Err(e);
                        break;
                    }

                    let module_id = ModuleId::new(package.0.into(), module);
                    adapter::execute(
                        move_vm,
                        temporary_store,
                        module_id,
                        &function,
                        type_arguments,
                        arguments,
                        gas_status.create_move_gas_status(),
                        tx_ctx,
                    )
                }
                SingleTransactionKind::Publish(MoveModulePublish { modules }) => {
                    // Charge gas for this VM execution
                    if let Err(e) = gas_status.charge_vm_gas() {
                        result = Err(e);
                        break;
                    }
                    // Charge gas for this publish
                    if let Err(e) =
                        gas_status.charge_publish_package(modules.iter().map(|v| v.len()).sum())
                    {
                        result = Err(e);
                        break;
                    }
                    adapter::publish(
                        temporary_store,
                        native_functions.clone(),
                        modules,
                        tx_ctx,
                        gas_status.create_move_gas_status(),
                    )
                }
                SingleTransactionKind::Pay(Pay {
                    coins,
                    recipients,
                    amounts,
                }) => {
                    let coin_objects =  // unwrap is is safe because we built the object map from the transaction
                    coins.iter().map(|c|
                    temporary_store
                        .objects()
                        .get(&c.0)
                        .unwrap()
                        .clone()
                    ).collect();
                    pay(temporary_store, coin_objects, recipients, amounts, tx_ctx)
                }
                SingleTransactionKind::ChangeEpoch(ChangeEpoch {
                    epoch,
                    storage_charge,
                    computation_charge,
                    storage_rebate,
                }) => {
                    let module_id =
                        ModuleId::new(SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_MODULE_NAME.to_owned());
                    let function = ADVANCE_EPOCH_FUNCTION_NAME.to_owned();
                    adapter::execute(
                        move_vm,
                        temporary_store,
                        module_id,
                        &function,
                        vec![],
                        vec![
                            CallArg::Object(ObjectArg::SharedObject {
                                id: SUI_SYSTEM_STATE_OBJECT_ID,
                                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                            }),
                            CallArg::Pure(bcs::to_bytes(&epoch).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&storage_charge).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&computation_charge).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&storage_rebate).unwrap()),
                        ],
                        gas_status.create_move_gas_status(),
                        tx_ctx,
                    )
                }
            };
            if result.is_err() {
                break;
            }
        }
        if result.is_err() {
            // Roll back the temporary store if execution failed.
            temporary_store.reset();
        }
    }

    // Make sure every mutable object's version number is incremented.
    // This needs to happen before `charge_gas_for_storage_changes` so that it
    // can charge gas for all mutated objects properly.
    temporary_store.ensure_active_inputs_mutated(&gas_object_id);
    if !gas_status.is_unmetered() {
        // We must call `read_object` instead of getting it from `temporary_store.objects`
        // because a `TransferSui` transaction may have already mutated the gas object and put
        // it in `temporary_store.written`.
        let mut gas_object = temporary_store
            .read_object(&gas_object_id)
            .expect("We constructed the object map so it should always have the gas object id")
            .clone();
        trace!(?gas_object_id, "Obtained gas object");
        if let Err(err) =
            temporary_store.charge_gas_for_storage_changes(&mut gas_status, &mut gas_object)
        {
            // If `result` is already `Err`, we basically have two errors at the same time.
            // Users should be generally more interested in the actual execution error, so we
            // let that shadow the out of gas error. Also in this case, we don't need to reset
            // the `temporary_store` because `charge_gas_for_storage_changes` won't mutate
            // `temporary_store` if gas charge failed.
            //
            // If `result` is `Ok`, now we failed when charging gas, we have to reset
            // the `temporary_store` to eliminate all effects caused by the execution,
            // and re-ensure all mutable objects' versions are incremented.
            if result.is_ok() {
                temporary_store.reset();
                temporary_store.ensure_active_inputs_mutated(&gas_object_id);
                result = Err(err);
            }
        }
        let cost_summary = gas_status.summary(result.is_ok());
        let gas_used = cost_summary.gas_used();
        // TODO: Only refund to user a percentage of the storage rebate. This percentage should be read
        // from a system parameter stored on-chain.
        let gas_rebate = cost_summary.storage_rebate;
        // We must re-fetch the gas object from the temporary store, as it may have been reset
        // previously in the case of error.
        // TODO: It might be cleaner and less error-prone if we put gas object id into
        // temporary store and move much of the gas logic there.
        gas_object = temporary_store.read_object(&gas_object_id).unwrap().clone();
        gas::deduct_gas(&mut gas_object, gas_used, gas_rebate);
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");
        temporary_store.write_object(gas_object, WriteKind::Mutate);
    }

    let cost_summary = gas_status.summary(result.is_ok());
    (cost_summary, result)
}

fn transfer_object<S>(
    temporary_store: &mut TemporaryStore<S>,
    mut object: Object,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    object.ensure_public_transfer_eligible()?;
    object.transfer_and_increment_version(recipient);
    // This will extract the transfer amount if the object is a Coin of some kind
    let amount = Coin::extract_balance_if_coin(&object)?;
    temporary_store.log_event(Event::TransferObject {
        package_id: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
        transaction_module: Identifier::from(ident_str!("native")),
        sender,
        recipient: Owner::AddressOwner(recipient),
        object_id: object.id(),
        version: object.version(),
        type_: TransferType::Coin,
        amount,
    });
    temporary_store.write_object(object, WriteKind::Mutate);
    Ok(())
}

fn check_coins(
    coin_objects: &[Object],
    mut coin_type: Option<StructTag>,
) -> Result<(Vec<Coin>, StructTag), ExecutionError> {
    if coin_objects.is_empty() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::EmptyInputCoins,
            "Pay transaction requires a non-empty list of input coins".to_string(),
        ));
    }
    let mut coins = Vec::new();
    for coin_obj in coin_objects {
        match &coin_obj.data {
            Data::Move(move_obj) => {
                if !Coin::is_coin(&move_obj.type_) {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvalidCoinObject,
                        "Provided non-Coin<T> object as input to pay/pay_sui/pay_all_sui transaction".to_string(),
                    ));
                }
                if let Some(typ) = &coin_type {
                    if typ != &move_obj.type_ {
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::CoinTypeMismatch,
                            format!("Coin type check failed in pay/pay_sui/pay_all_sui transaction, expected: {:?}, found: {:}", typ, move_obj.type_),
                        ));
                    }
                } else {
                    coin_type = Some(move_obj.type_.clone())
                }

                let coin = Coin::from_bcs_bytes(move_obj.contents())
                    .expect("Deserializing coin object should not fail");
                coins.push(coin)
            }
            _ => {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::InvalidCoinObject,
                    "Provided non-Coin<T> object as input to pay transaction".to_string(),
                ))
            }
        }
    }
    // safe because coin_objects must be non-empty, and coin_type must be set in loop above.
    Ok((coins, coin_type.unwrap()))
}

fn check_recipients(recipients: &[SuiAddress], amounts: &[u64]) -> Result<(), ExecutionError> {
    if recipients.is_empty() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::EmptyRecipients,
            "Pay transaction requires a non-empty list of recipient addresses".to_string(),
        ));
    }
    if recipients.len() != amounts.len() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::RecipientsAmountsArityMismatch,
            format!(
                "Found {:?} recipient addresses, but {:?} recipient amounts",
                recipients.len(),
                amounts.len()
            ),
        ));
    }
    Ok(())
}

fn check_total_coins(coins: &[Coin], amounts: &[u64]) -> Result<(u64, u64), ExecutionError> {
    let total_amount: u64 = amounts.iter().sum();
    let total_coins = coins.iter().fold(0, |acc, c| acc + c.value());
    if total_amount > total_coins {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::InsufficientBalance,
            format!("Attempting to pay a total amount {:?} that is greater than the sum of input coin values {:?}", total_amount, total_coins),
        ));
    }
    Ok((total_coins, total_amount))
}

fn debit_coins_and_transfer<S>(
    temporary_store: &mut TemporaryStore<S>,
    coins: &mut [Coin],
    recipients: &[SuiAddress],
    amounts: &[u64],
    coin_type: StructTag,
    tx_ctx: &mut TxContext,
) -> usize {
    let mut cur_coin_idx = 0;
    for (recipient, amount) in recipients.iter().zip(amounts) {
        let mut remaining_amount = *amount;
        loop {
            // while remaining_amount != 0
            // guaranteed to be in-bounds because of the total > total_coins check above
            let coin = &mut coins[cur_coin_idx];
            let coin_value = coin.value();
            if coin_value == 0 {
                // can't extract any more value from this coin, go to the next one
                cur_coin_idx += 1;
                continue;
            }
            if coin_value >= remaining_amount {
                // can get everything we need from this coin
                coin.balance.withdraw(remaining_amount).unwrap();
                // create a new coin for the recipient with the original amount
                let new_coin = Object::new_move(
                    MoveObject::new_coin(
                        coin_type.clone(),
                        OBJECT_START_VERSION,
                        bcs::to_bytes(&Coin::new(UID::new(tx_ctx.fresh_id()), *amount))
                            .expect("Serializing coin value cannot fail"),
                    ),
                    Owner::AddressOwner(*recipient),
                    tx_ctx.digest(),
                );
                temporary_store.log_event(Event::TransferObject {
                    package_id: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
                    transaction_module: Identifier::from(ident_str!("native")),
                    sender: tx_ctx.sender(),
                    recipient: Owner::AddressOwner(*recipient),
                    object_id: new_coin.id(),
                    version: new_coin.version(),
                    type_: TransferType::Coin, // Should this be a separate type, like SuiCoin?
                    amount: Some(amount),
                });

                temporary_store.write_object(new_coin, WriteKind::Create);
                break; // done paying this recipieint, on to the next one
            } else {
                // need to take all of this coin and some from a subsequent coin
                coin.balance.withdraw(coin_value).unwrap();
                remaining_amount -= coin_value;
            }
        }
    }
    cur_coin_idx
}

/// Debit `coins` to pay amount[i] to recipient[i]. The coins are debited from left to right.
/// A new coin object is created for each recipient.
fn pay<S>(
    temporary_store: &mut TemporaryStore<S>,
    coin_objects: Vec<Object>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    tx_ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    check_recipients(&recipients, &amounts)?;
    let (mut coins, coin_type) = check_coins(&coin_objects, None)?;
    let (total_coins, total_amount) = check_total_coins(&coins, &amounts)?;
    debit_coins_and_transfer(
        temporary_store,
        &mut coins,
        &recipients,
        &amounts,
        coin_type,
        tx_ctx,
    );

    #[cfg(debug_assertions)]
    {
        // double check that we didn't create or destroy money
        let new_total_coins = coins.iter().fold(0, |acc, c| acc + c.value());
        assert_eq!(total_coins - new_total_coins, total_amount)
    }

    // update the input coins to reflect the decrease in value.
    // if the input coin has value 0, delete it
    for (coin_idx, mut coin_object) in coin_objects.into_iter().enumerate() {
        let coin = &coins[coin_idx];
        if coin.value() == 0 {
            temporary_store.delete_object(
                &coin_object.id(),
                coin_object.version(),
                DeleteKind::Normal,
            )
        } else {
            // unwrapped unsafe because we checked that it was a coin object above
            coin_object
                .data
                .try_as_move_mut()
                .unwrap()
                .update_contents_and_increment_version(
                    bcs::to_bytes(&coin).expect("Coin serialization should not fail"),
                );
            temporary_store.write_object(coin_object, WriteKind::Mutate);
        }
    }
    Ok(())
}

fn pay_sui<S>(
    temporary_store: &mut TemporaryStore<S>,
    coin_objects: &mut Vec<Object>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    tx_ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    let (mut coins, coin_type) = check_coins(coin_objects, Some(GasCoin::type_()))?;
    check_recipients(&recipients, &amounts)?;
    let (total_coins, total_amount) = check_total_coins(&coins, &amounts)?;

    let mut merged_coin = coins.swap_remove(0);
    merged_coin.merge_coins(&mut coins);

    for (recipient, amount) in recipients.iter().zip(amounts) {
        // unwrap is safe b/c merged_coin value is total_coins, which is greater than total_amount
        let new_coin = merged_coin
            .split_coin(amount, UID::new(tx_ctx.fresh_id()))
            .unwrap();
        transfer_coin(
            temporary_store,
            &new_coin,
            *recipient,
            coin_type.clone(),
            tx_ctx,
        );
    }
    update_input_coins(temporary_store, coin_objects, &merged_coin, None);

    #[cfg(debug_assertions)]
    {
        assert_eq!(total_coins - merged_coin.value(), total_amount)
    }
    Ok(())
}

fn pay_all_sui<S>(
    temporary_store: &mut TemporaryStore<S>,
    coin_objects: &mut Vec<Object>,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    let (mut coins, _coin_type) = check_coins(coin_objects, Some(GasCoin::type_()))?;
    let total_coins = coins.iter().fold(0, |acc, c| acc + c.value());

    let mut merged_coin = coins.swap_remove(0);
    merged_coin.merge_coins(&mut coins);
    update_input_coins(temporary_store, coin_objects, &merged_coin, Some(recipient));

    #[cfg(debug_assertions)]
    {
        assert_eq!(total_coins, merged_coin.value());
    }
    Ok(())
}

/// Transfer the gas object (which is a SUI coin object) with an optional `amount`.
/// If `amount` is specified, the gas object remains in the original owner, but a new SUI coin
/// is created with `amount` balance and is transferred to `recipient`;
/// if `amount` is not specified, the entire object will be transferred to `recipient`.
/// `tx_ctx` is needed to create new object ID for the split coin.
/// We make sure that the gas object's version is not incremented after this function call, because
/// when we charge gas later, its version will be officially incremented.
fn transfer_sui<S>(
    temporary_store: &mut TemporaryStore<S>,
    mut object: Object,
    recipient: SuiAddress,
    amount: Option<u64>,
    tx_ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    #[cfg(debug_assertions)]
    let version = object.version();

    let transferred = if let Some(amount) = amount {
        // Deduct the amount from the gas coin and update it.
        let mut gas_coin = GasCoin::try_from(&object)
            .expect("gas object is transferred, so already checked to be a SUI coin");
        gas_coin.0.balance.withdraw(amount)?;
        let move_object = object
            .data
            .try_as_move_mut()
            .expect("Gas object must be Move object");
        // We do not update the version number yet because gas charge will update it latter.
        move_object.update_contents_without_version_change(
            bcs::to_bytes(&gas_coin).expect("Serializing gas coin can never fail"),
        );

        // Creat a new gas coin with the amount.
        let new_object = Object::new_move(
            MoveObject::new_gas_coin(
                OBJECT_START_VERSION,
                bcs::to_bytes(&GasCoin::new(tx_ctx.fresh_id(), amount))
                    .expect("Serializing gas object cannot fail"),
            ),
            Owner::AddressOwner(recipient),
            tx_ctx.digest(),
        );
        temporary_store.write_object(new_object, WriteKind::Create);
        Some(amount)
    } else {
        // If amount is not specified, we simply transfer the entire coin object.
        // We don't want to increment the version number yet because latter gas charge will do it.
        object.transfer_without_version_change(recipient);
        Coin::extract_balance_if_coin(&object)?
    };

    temporary_store.log_event(Event::TransferObject {
        package_id: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
        transaction_module: Identifier::from(ident_str!("native")),
        sender: tx_ctx.sender(),
        recipient: Owner::AddressOwner(recipient),
        object_id: object.id(),
        version: object.version(),
        type_: TransferType::Coin, // Should this be a separate type, like SuiCoin?
        amount: transferred,
    });

    #[cfg(debug_assertions)]
    assert_eq!(object.version(), version);

    temporary_store.write_object(object, WriteKind::Mutate);

    Ok(())
}

#[test]
fn test_pay_empty_coins() {
    let coin_objects = Vec::new();
    let recipients = vec![SuiAddress::random_for_testing_only()];
    let amounts = vec![10];
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::EmptyInputCoins
    );
}

#[test]
fn test_pay_empty_recipients() {
    let coin_objects = vec![Object::new_gas_coin_for_testing(
        10,
        SuiAddress::random_for_testing_only(),
    )];
    let recipients = Vec::new();
    let amounts = vec![10];
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::EmptyRecipients
    );
}

#[test]
fn test_pay_empty_amounts() {
    let coin_objects = vec![Object::new_gas_coin_for_testing(
        10,
        SuiAddress::random_for_testing_only(),
    )];
    let recipients = vec![SuiAddress::random_for_testing_only()];
    let amounts = Vec::new();
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::RecipientsAmountsArityMismatch
    );
}

#[test]
fn test_pay_arity_mismatch() {
    // different number of recipients and amounts
    let owner = SuiAddress::random_for_testing_only();
    let coin_objects = vec![Object::new_gas_coin_for_testing(10, owner)];
    let recipients = vec![
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    ];
    let amounts = vec![5];
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::RecipientsAmountsArityMismatch
    );
}

#[test]
fn test_pay_insufficient_balance() {
    let coin_objects = vec![
        Object::new_gas_coin_for_testing(10, SuiAddress::random_for_testing_only()),
        Object::new_gas_coin_for_testing(5, SuiAddress::random_for_testing_only()),
    ];
    let recipients = vec![
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    ];
    let amounts = vec![10, 6];
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::InsufficientBalance
    );
}

#[cfg(test)]
fn get_coin_balance(store: &InnerTemporaryStore, id: &ObjectID) -> u64 {
    Coin::extract_balance_if_coin(store.get_written_object(id).unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn test_pay_success_without_delete() {
    // supplied one coin and only needed to use part of it. should
    // mutate 1 object, create 1 object, and delete no objects
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_coin_for_testing(10, sender);
    let coin2 = Object::new_gas_coin_for_testing(5, sender);
    let coin_objects = vec![coin1, coin2];
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![6, 3];
    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    let mut ctx = TxContext::with_sender_for_testing_only(&sender);

    assert!(pay(&mut store, coin_objects, recipients, amounts, &mut ctx).is_ok());
    let (store, _events) = store.into_inner();

    assert!(store.deleted.is_empty());
    assert_eq!(store.created().len(), 2);
    let recipient1_objs = store.get_written_objects_owned_by(&recipient1);
    let recipient2_objs = store.get_written_objects_owned_by(&recipient2);
    assert_eq!(recipient1_objs.len(), 1);
    assert_eq!(recipient2_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &recipient1_objs[0]), 6);
    assert_eq!(get_coin_balance(&store, &recipient2_objs[0]), 3);

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert_eq!(owner_objs.len(), 2);
    assert_eq!(
        get_coin_balance(&store, &owner_objs[0]) + get_coin_balance(&store, &owner_objs[1]),
        6
    );
}

#[test]
fn test_pay_success_delete_one() {
    // supplied two coins, spent all of the first one and some of the second one
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_coin_for_testing(10, sender);
    let coin2 = Object::new_gas_coin_for_testing(5, sender);
    let input_coin_id1 = coin1.id();
    let input_coin_id2 = coin2.id();
    let coin_objects = vec![coin1, coin2];
    let recipient = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient];
    let amounts = vec![11];
    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    let mut ctx = TxContext::random_for_testing_only();

    assert!(pay(&mut store, coin_objects, recipients, amounts, &mut ctx).is_ok());
    let (store, _events) = store.into_inner();

    assert_eq!(store.deleted.len(), 1);
    assert!(store.deleted.contains_key(&input_coin_id1));

    assert_eq!(store.written.len(), 2);
    assert_eq!(store.created().len(), 1);
    let recipient_objs = store.get_written_objects_owned_by(&recipient);
    assert_eq!(recipient_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &recipient_objs[0]), 11);

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert_eq!(owner_objs.len(), 1);
    assert_eq!(owner_objs[0], input_coin_id2);
    assert_eq!(get_coin_balance(&store, &owner_objs[0]), 4);
}

#[test]
fn test_pay_success_delete_all() {
    // supplied two coins, spent both of them
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_coin_for_testing(10, sender);
    let coin2 = Object::new_gas_coin_for_testing(5, sender);
    let input_coin_id1 = coin1.id();
    let input_coin_id2 = coin2.id();
    let coin_objects = vec![coin1, coin2];
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![4, 11];
    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    let mut ctx = TxContext::with_sender_for_testing_only(&sender);

    assert!(pay(&mut store, coin_objects, recipients, amounts, &mut ctx).is_ok());
    let (store, _events) = store.into_inner();

    assert_eq!(store.deleted.len(), 2);
    assert!(store.deleted.contains_key(&input_coin_id1));
    assert!(store.deleted.contains_key(&input_coin_id2));

    assert_eq!(store.written.len(), 2);
    assert_eq!(store.created().len(), 2);
    let recipient1_objs = store.get_written_objects_owned_by(&recipient1);
    let recipient2_objs = store.get_written_objects_owned_by(&recipient2);
    assert_eq!(recipient1_objs.len(), 1);
    assert_eq!(recipient2_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &recipient1_objs[0]), 4);
    assert_eq!(get_coin_balance(&store, &recipient2_objs[0]), 11);

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert!(owner_objs.is_empty());
}

#[test]
fn test_pay_sui_success_one_input_coin() {
    let sender = SuiAddress::random_for_testing_only();
    let coin = Object::new_gas_coin_for_testing(18, sender);
    let mut coin_objects = vec![coin];

    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![8, 6];

    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    let mut ctx = TxContext::with_sender_for_testing_only(&sender);
    assert!(pay_sui(&mut store, &mut coin_objects, recipients, amounts, &mut ctx).is_ok());
    let (store, _events) = store.into_inner();

    assert!(store.deleted.is_empty());
    assert_eq!(store.written.len(), 3);
    assert_eq!(store.created().len(), 2);

    let recipient1_objs = store.get_written_objects_owned_by(&recipient1);
    let recipient2_objs = store.get_written_objects_owned_by(&recipient2);
    assert_eq!(recipient1_objs.len(), 1);
    assert_eq!(recipient2_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &recipient1_objs[0]), 8);
    assert_eq!(get_coin_balance(&store, &recipient2_objs[0]), 6);

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert_eq!(owner_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &owner_objs[0]), 4);
}

#[test]
fn test_pay_sui_success_multiple_input_coins() {
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_coin_for_testing(30, sender);
    let coin2 = Object::new_gas_coin_for_testing(20, sender);
    let coin3 = Object::new_gas_coin_for_testing(10, sender);
    let input_coin_id2 = coin2.id();
    let input_coin_id3 = coin3.id();
    let mut coin_objects = vec![coin1, coin2, coin3];

    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipient3 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2, recipient3];
    let amounts = vec![5, 15, 25];

    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    let mut ctx = TxContext::with_sender_for_testing_only(&sender);
    assert!(pay_sui(&mut store, &mut coin_objects, recipients, amounts, &mut ctx).is_ok());
    let (store, _events) = store.into_inner();

    assert_eq!(store.deleted.len(), 2);
    assert!(store.deleted.contains_key(&input_coin_id2));
    assert!(store.deleted.contains_key(&input_coin_id3));
    assert_eq!(store.written.len(), 4);
    assert_eq!(store.created().len(), 3);

    let recipient1_objs = store.get_written_objects_owned_by(&recipient1);
    let recipient2_objs = store.get_written_objects_owned_by(&recipient2);
    let recipient3_objs = store.get_written_objects_owned_by(&recipient3);
    assert_eq!(recipient1_objs.len(), 1);
    assert_eq!(recipient2_objs.len(), 1);
    assert_eq!(recipient3_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &recipient1_objs[0]), 5);
    assert_eq!(get_coin_balance(&store, &recipient2_objs[0]), 15);
    assert_eq!(get_coin_balance(&store, &recipient3_objs[0]), 25);

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert_eq!(owner_objs.len(), 1);
    assert_eq!(get_coin_balance(&store, &owner_objs[0]), 15);
}

#[test]
fn test_pay_all_sui_success_multiple_input_coins() {
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_coin_for_testing(30, sender);
    let coin2 = Object::new_gas_coin_for_testing(20, sender);
    let coin3 = Object::new_gas_coin_for_testing(10, sender);
    let input_coin_id2 = coin2.id();
    let input_coin_id3 = coin3.id();
    let mut coin_objects = vec![coin1, coin2, coin3];

    let recipient = SuiAddress::random_for_testing_only();

    let mut store: TemporaryStore<()> =
        temporary_store::with_input_objects_for_testing(InputObjects::from(coin_objects.clone()));
    assert!(pay_all_sui(&mut store, &mut coin_objects, recipient).is_ok());
    let (store, _events) = store.into_inner();

    assert_eq!(store.deleted.len(), 2);
    assert!(store.deleted.contains_key(&input_coin_id2));
    assert!(store.deleted.contains_key(&input_coin_id3));
    assert_eq!(store.written.len(), 1);
    assert!(store.created().is_empty());

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert!(owner_objs.is_empty());
}
