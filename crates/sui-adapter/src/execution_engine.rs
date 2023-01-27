// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::execution_mode::{self, ExecutionMode};
use move_core_types::language_storage::{ModuleId, StructTag};
use move_vm_runtime::move_vm::MoveVM;
use sui_types::base_types::SequenceNumber;
use tracing::{debug, instrument};

use crate::adapter;
use sui_protocol_config::ProtocolConfig;
use sui_types::coin::{transfer_coin, update_input_coins, Coin};
use sui_types::epoch_data::EpochData;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::id::UID;
#[cfg(test)]
use sui_types::messages::ExecutionFailureStatus;
#[cfg(test)]
use sui_types::messages::InputObjects;
use sui_types::messages::{
    ConsensusCommitPrologue, GenesisTransaction, ObjectArg, Pay, PayAllSui, PaySui, TransactionKind,
};
use sui_types::object::{Data, MoveObject, Owner};
use sui_types::storage::SingleTxContext;
use sui_types::storage::{ChildObjectResolver, DeleteKind, ParentSync, WriteKind};
use sui_types::sui_system_state::{
    ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME, CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME,
};
#[cfg(test)]
use sui_types::temporary_store;
use sui_types::temporary_store::InnerTemporaryStore;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, TxContext},
    gas::SuiGasStatus,
    messages::{
        CallArg, ChangeEpoch, ExecutionStatus, MoveCall, MoveModulePublish, SingleTransactionKind,
        TransactionEffects, TransferObject, TransferSui,
    },
    object::Object,
    storage::BackingPackageStore,
    sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{
    MOVE_STDLIB_OBJECT_ID, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

use sui_types::temporary_store::TemporaryStore;

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver,
>(
    shared_object_refs: Vec<ObjectRef>,
    mut temporary_store: TemporaryStore<S>,
    transaction_kind: TransactionKind,
    transaction_signer: SuiAddress,
    gas: &[ObjectRef],
    transaction_digest: TransactionDigest,
    mut transaction_dependencies: BTreeSet<TransactionDigest>,
    move_vm: &Arc<MoveVM>,
    gas_status: SuiGasStatus,
    epoch_data: &EpochData,
    protocol_config: &ProtocolConfig,
) -> (
    InnerTemporaryStore,
    TransactionEffects,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    let mut tx_ctx = TxContext::new(&transaction_signer, &transaction_digest, epoch_data);

    // Combine (smash) gas here
    let (gas_ref, result) = if !gas_status.is_unmetered() && gas.len() > 1 {
        match smash_gas_coins(&tx_ctx, gas, &mut temporary_store) {
            Ok(obj_ref) => (obj_ref, Ok(())),
            // this should not happen given we have a certificate already, and we chacked in the caller,
            // so we assume that vector of coins for gas is not empty and the first object
            // can be used for gas
            Err(err) => (gas[0], Err(err)),
        }
    } else {
        (gas[0], Ok(()))
    };
    let (gas_cost_summary, execution_result) = match result {
        Ok(_) => execute_transaction::<Mode, _>(
            &mut temporary_store,
            transaction_kind,
            gas_ref.0,
            &mut tx_ctx,
            move_vm,
            gas_status,
            protocol_config,
        ),
        Err(err) => (gas_status.summary(false), Err(err)),
    };

    let (status, execution_result) = match execution_result {
        Ok(results) => (ExecutionStatus::Success, Ok(results)),
        Err(error) => (
            ExecutionStatus::new_failure(error.to_execution_status()),
            Err(error),
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
        gas_ref,
        epoch_data.epoch_id(),
    );
    (inner, effects, execution_result)
}

fn smash_gas_coins<S>(
    tx_ctx: &TxContext,
    gas_object_refs: &[ObjectRef],
    temporary_store: &mut TemporaryStore<S>,
) -> Result<ObjectRef, ExecutionError> {
    if gas_object_refs.len() == 1 {
        return Ok(gas_object_refs[0]);
    }
    let mut gas_coins: Vec<Object> =  // unwrap is safe because we built the object map from the transaction
        gas_object_refs.iter().map(|obj_ref|
            temporary_store
                .objects()
                .get(&obj_ref.0)
                .unwrap()
                .clone()
        ).collect();
    check_coins(&gas_coins, Some(GasCoin::type_())).map(|(mut coins, _)| {
        let mut merged_coin = coins.swap_remove(0);
        // TODO: manage error
        merged_coin.merge_coins(&mut coins);
        gas_coins.swap_remove(0);
        update_input_coins(
            // making this feels so idiotic and is it even right?
            &SingleTxContext::gas(tx_ctx.sender()),
            temporary_store,
            &mut gas_coins,
            &merged_coin,
            None,
        );
        gas_object_refs[0]
    })
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
        .iter()
        // don't charge for loading Sui Framework or Move stdlib
        .filter(|(id, _)| *id != &SUI_FRAMEWORK_OBJECT_ID && *id != &MOVE_STDLIB_OBJECT_ID)
        .map(|(_, obj)| obj.object_size_for_gas_metering())
        .sum();
    gas_status.charge_storage_read(total_size)
}

#[instrument(name = "tx_execute", level = "debug", skip_all)]
fn execute_transaction<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver,
>(
    temporary_store: &mut TemporaryStore<S>,
    transaction_kind: TransactionKind,
    gas_object_id: ObjectID,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    mut gas_status: SuiGasStatus,
    protocol_config: &ProtocolConfig,
) -> (
    GasCostSummary,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    // We must charge object read gas inside here during transaction execution, because if this fails
    // we must still ensure an effect is committed and all objects versions incremented.
    let result = charge_gas_for_object_read(temporary_store, &mut gas_status);
    let mut result = result.and_then(|()| {
        let execution_result = execution_loop::<Mode, _>(
            temporary_store,
            transaction_kind,
            gas_object_id,
            tx_ctx,
            move_vm,
            &mut gas_status,
            protocol_config,
        );
        if execution_result.is_err() {
            // Roll back the temporary store if execution failed.
            temporary_store.reset();
        }
        execution_result
    });

    // Make sure every mutable object's version number is incremented.
    // This needs to happen before `charge_gas_for_storage_changes` so that it
    // can charge gas for all mutated objects properly.
    let sender = tx_ctx.sender();
    temporary_store.ensure_active_inputs_mutated(sender, &gas_object_id);
    if !gas_status.is_unmetered() {
        temporary_store.charge_gas(sender, gas_object_id, &mut gas_status, &mut result);
    }

    let cost_summary = gas_status.summary(result.is_ok());
    (cost_summary, result)
}

fn execution_loop<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver,
>(
    temporary_store: &mut TemporaryStore<S>,
    transaction_kind: TransactionKind,
    gas_object_id: ObjectID,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
) -> Result<Mode::ExecutionResults, ExecutionError> {
    let mut results = Mode::empty_results();
    // TODO: Since we require all mutable objects to not show up more than
    // once across single tx, we should be able to run them in parallel.
    for (idx, single_tx) in transaction_kind.into_single_transactions().enumerate() {
        match single_tx {
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
                transfer_object(temporary_store, object, tx_ctx.sender(), recipient)?;
            }
            SingleTransactionKind::TransferSui(TransferSui { recipient, amount }) => {
                let gas_object = temporary_store
                    .objects()
                    .get(&gas_object_id)
                    .expect(
                        "We constructed the object map so it should always have the gas object id",
                    )
                    .clone();
                transfer_sui(temporary_store, gas_object, recipient, amount, tx_ctx)?;
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
                )?;
            }
            SingleTransactionKind::PayAllSui(PayAllSui { coins, recipient }) => {
                // unwrap is is safe because we built the object map from the transaction
                let mut coin_objects: Vec<Object> = coins
                    .iter()
                    .map(|c| temporary_store.objects().get(&c.0).unwrap().clone())
                    .collect();
                pay_all_sui(
                    tx_ctx.sender(),
                    temporary_store,
                    &mut coin_objects,
                    recipient,
                )?;
            }
            SingleTransactionKind::Call(MoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            }) => {
                // Charge gas for this VM execution
                gas_status.charge_vm_gas()?;

                let module_id = ModuleId::new(package.into(), module);
                let result = adapter::execute::<Mode, _, _>(
                    move_vm,
                    temporary_store,
                    module_id,
                    &function,
                    type_arguments,
                    arguments,
                    gas_status.create_move_gas_status(),
                    tx_ctx,
                    protocol_config,
                )?;
                Mode::add_result(&mut results, idx, result);
            }
            SingleTransactionKind::Publish(MoveModulePublish { modules }) => {
                // Charge gas for this VM execution
                gas_status.charge_vm_gas()?;
                // Charge gas for this publish
                gas_status.charge_publish_package(modules.iter().map(|v| v.len()).sum())?;
                adapter::publish(
                    temporary_store,
                    move_vm,
                    modules,
                    tx_ctx,
                    gas_status.create_move_gas_status(),
                    protocol_config,
                )?;
            }
            SingleTransactionKind::Pay(Pay {
                coins,
                recipients,
                amounts,
            }) => {
                // unwrap is is safe because we built the object map from the transaction
                let coin_objects = coins
                    .iter()
                    .map(|c| temporary_store.objects().get(&c.0).unwrap().clone())
                    .collect();
                pay(temporary_store, coin_objects, recipients, amounts, tx_ctx)?;
            }
            SingleTransactionKind::ChangeEpoch(change_epoch) => {
                advance_epoch(
                    change_epoch,
                    temporary_store,
                    tx_ctx,
                    move_vm,
                    gas_status,
                    protocol_config,
                )?;
            }
            SingleTransactionKind::Genesis(GenesisTransaction { objects }) => {
                if tx_ctx.epoch() != 0 {
                    panic!("BUG: Genesis Transactions can only be executed in epoch 0");
                }

                for genesis_object in objects {
                    match genesis_object {
                        sui_types::messages::GenesisObject::RawObject { data, owner } => {
                            let object = Object {
                                data,
                                owner,
                                previous_transaction: tx_ctx.digest(),
                                storage_rebate: 0,
                            };
                            temporary_store.write_object(
                                &SingleTxContext::genesis(),
                                object,
                                WriteKind::Create,
                            );
                        }
                    }
                }
            }
            SingleTransactionKind::ConsensusCommitPrologue(prologue) => setup_consensus_commit(
                prologue,
                temporary_store,
                tx_ctx,
                move_vm,
                gas_status,
                protocol_config,
            )?,
            SingleTransactionKind::ProgrammableTransaction(_) => {
                unreachable!("programmable transactions are not yet supported")
            }
        };
    }
    Ok(results)
}

fn advance_epoch<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    change_epoch: ChangeEpoch,
    temporary_store: &mut TemporaryStore<S>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
) -> Result<(), ExecutionError> {
    let module_id = ModuleId::new(SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_MODULE_NAME.to_owned());
    let function = ADVANCE_EPOCH_FUNCTION_NAME.to_owned();
    let system_object_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });
    let result = adapter::execute::<execution_mode::Normal, _, _>(
        move_vm,
        temporary_store,
        module_id.clone(),
        &function,
        vec![],
        vec![
            system_object_arg.clone(),
            CallArg::Pure(bcs::to_bytes(&change_epoch.epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.protocol_version).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.storage_charge).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.computation_charge).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.storage_rebate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&protocol_config.storage_fund_reinvest_rate()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&protocol_config.reward_slashing_rate()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&protocol_config.stake_subsidy_rate()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.epoch_start_timestamp_ms).unwrap()),
        ],
        gas_status.create_move_gas_status(),
        tx_ctx,
        protocol_config,
    );
    if result.is_err() {
        tracing::error!(
            "Failed to execute advance epoch transaction. Switching to safe mode. Error: {:?}. System state object: {:?}. Tx data: {:?}",
            result.as_ref().err(),
            temporary_store.read_object(&SUI_SYSTEM_STATE_OBJECT_ID),
            change_epoch,
        );
        temporary_store.reset();
        let function = ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME.to_owned();
        adapter::execute::<execution_mode::Normal, _, _>(
            move_vm,
            temporary_store,
            module_id,
            &function,
            vec![],
            vec![
                system_object_arg,
                CallArg::Pure(bcs::to_bytes(&change_epoch.epoch).unwrap()),
                CallArg::Pure(bcs::to_bytes(&change_epoch.protocol_version).unwrap()),
            ],
            gas_status.create_move_gas_status(),
            tx_ctx,
            protocol_config,
        )?;
    }
    Ok(())
}

/// Perform metadata updates in preparation for the transactions in the upcoming checkpoint:
///
/// - Set the timestamp for the `Clock` shared object from the timestamp in the header from
///   consensus.
fn setup_consensus_commit<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    prologue: ConsensusCommitPrologue,
    temporary_store: &mut TemporaryStore<S>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
) -> Result<(), ExecutionError> {
    adapter::execute::<execution_mode::Normal, _, _>(
        move_vm,
        temporary_store,
        ModuleId::new(SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_MODULE_NAME.to_owned()),
        &CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME.to_owned(),
        vec![],
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_CLOCK_OBJECT_ID,
                initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                mutable: true,
            }),
            CallArg::Pure(bcs::to_bytes(&prologue.checkpoint_start_timestamp_ms).unwrap()),
        ],
        gas_status.create_move_gas_status(),
        tx_ctx,
        protocol_config,
    )?;

    Ok(())
}

fn transfer_object<S>(
    temporary_store: &mut TemporaryStore<S>,
    mut object: Object,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    object.ensure_public_transfer_eligible()?;
    object.transfer(recipient);
    // This will extract the transfer amount if the object is a Coin of some kind
    let ctx = SingleTxContext::transfer_object(sender);
    temporary_store.write_object(&ctx, object, WriteKind::Mutate);
    Ok(())
}

fn check_coins(
    coin_objects: &[Object],
    mut coin_type: Option<StructTag>,
) -> Result<(Vec<Coin>, StructTag), ExecutionError> {
    if coin_objects.is_empty() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::EmptyInputCoins,
            "Transaction requires a non-empty list of input coins".to_string(),
        ));
    }
    let mut coins = Vec::new();
    for coin_obj in coin_objects {
        match &coin_obj.data {
            Data::Move(move_obj) => {
                if !Coin::is_coin(&move_obj.type_) {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvalidCoinObject,
                        "Provided non-Coin<T> object as input to transaction".to_string(),
                    ));
                }
                if let Some(typ) = &coin_type {
                    if typ != &move_obj.type_ {
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::CoinTypeMismatch,
                            format!(
                                "Coin type check failed in transaction, expected: {:?}, found: {:}",
                                typ, move_obj.type_
                            ),
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
                    "Provided non-Coin<T> object as input to transaction".to_string(),
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
    let Some(total_amount) = amounts.iter().fold(Some(0u64), |acc, a| acc?.checked_add(*a)) else {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::TotalAmountOverflow,
            "Attempting to pay a total amount that overflows u64".to_string(),
        ));
    };
    // u64 overflow is impossible because the sum of all coin values is bounded by the total amount
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
    ctx: &SingleTxContext,
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
            if remaining_amount == 0 {
                break; // nothing to pay
            }
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
                        SequenceNumber::new(),
                        tx_ctx.fresh_id(),
                        *amount,
                    ),
                    Owner::AddressOwner(*recipient),
                    tx_ctx.digest(),
                );
                temporary_store.write_object(ctx, new_coin, WriteKind::Create);
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
    let ctx = SingleTxContext::pay(tx_ctx.sender());

    debit_coins_and_transfer(
        &ctx,
        temporary_store,
        &mut coins,
        &recipients,
        &amounts,
        coin_type,
        tx_ctx,
    );

    // double check that we didn't create or destroy money
    // u64 overflow is impossible because the sum of all coin values is bounded by the total amount
    let left_coins = coins.iter().fold(0, |acc, c| acc + c.value());
    debug_assert!(left_coins <= total_coins);
    debug_assert_eq!(total_coins - left_coins, total_amount);

    // update the input coins to reflect the decrease in value.
    // if the input coin has value 0, delete it
    for (coin_idx, mut coin_object) in coin_objects.into_iter().enumerate() {
        let coin = &coins[coin_idx];
        if coin.value() == 0 {
            temporary_store.delete_object(
                &ctx,
                &coin_object.id(),
                coin_object.version(),
                DeleteKind::Normal,
            );
        } else {
            let new_contents = bcs::to_bytes(&coin).expect("Coin serialization should not fail");
            // unwrap safe because we checked that it was a coin object above
            let move_obj = coin_object.data.try_as_move_mut().unwrap();
            move_obj.update_coin_contents(new_contents);
            temporary_store.write_object(&ctx, coin_object, WriteKind::Mutate);
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
    merged_coin.merge_coins(&mut coins)?;

    let ctx = SingleTxContext::pay_sui(tx_ctx.sender());

    for (recipient, amount) in recipients.iter().zip(amounts) {
        // unwrap is safe b/c merged_coin value is total_coins, which is greater than total_amount
        let new_coin = merged_coin
            .split_coin(amount, UID::new(tx_ctx.fresh_id()))
            .unwrap();
        transfer_coin(
            &ctx,
            temporary_store,
            &new_coin,
            *recipient,
            coin_type.clone(),
            tx_ctx.digest(),
        );
    }
    update_input_coins(&ctx, temporary_store, coin_objects, &merged_coin, None);

    debug_assert_eq!(total_coins - merged_coin.value(), total_amount);
    Ok(())
}

fn pay_all_sui<S>(
    sender: SuiAddress,
    temporary_store: &mut TemporaryStore<S>,
    coin_objects: &mut Vec<Object>,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    let (mut coins, _coin_type) = check_coins(coin_objects, Some(GasCoin::type_()))?;
    let total_coins = coins.iter().fold(0, |acc, c| acc + c.value());

    let mut merged_coin = coins.swap_remove(0);
    merged_coin.merge_coins(&mut coins)?;
    let ctx = SingleTxContext::pay_all_sui(sender);
    update_input_coins(
        &ctx,
        temporary_store,
        coin_objects,
        &merged_coin,
        Some(recipient),
    );

    debug_assert_eq!(total_coins, merged_coin.value());
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
    let ctx = SingleTxContext::transfer_sui(tx_ctx.sender());
    if let Some(amount) = amount {
        // Deduct the amount from the gas coin and update it.
        let mut gas_coin = GasCoin::try_from(&object)
            .expect("gas object is transferred, so already checked to be a SUI coin");
        gas_coin.0.balance.withdraw(amount)?;
        let move_object = object
            .data
            .try_as_move_mut()
            .expect("Gas object must be Move object");
        let new_contents = bcs::to_bytes(&gas_coin).expect("Serializing gas coin can never fail");
        move_object.update_coin_contents(new_contents);

        // Creat a new gas coin with the amount.  Set a blank version, to be updated by the store
        // when it is committed to effects.
        let new_object = Object::new_move(
            MoveObject::new_gas_coin(SequenceNumber::new(), tx_ctx.fresh_id(), amount),
            Owner::AddressOwner(recipient),
            tx_ctx.digest(),
        );
        temporary_store.write_object(&ctx, new_object, WriteKind::Create);
        Some(amount)
    } else {
        // If amount is not specified, we simply transfer the entire coin object.
        object.transfer(recipient);
        Coin::extract_balance_if_coin(&object)?
    };

    #[cfg(debug_assertions)]
    assert_eq!(object.version(), version);

    temporary_store.write_object(&ctx, object, WriteKind::Mutate);

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
    let coin_objects = vec![Object::new_gas_with_balance_and_owner_for_testing(
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
    let coin_objects = vec![Object::new_gas_with_balance_and_owner_for_testing(
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
    let coin_objects = vec![Object::new_gas_with_balance_and_owner_for_testing(
        10, owner,
    )];
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
fn test_pay_amount_overflow() {
    let coin_objects = vec![Object::new_gas_with_balance_and_owner_for_testing(
        10,
        SuiAddress::random_for_testing_only(),
    )];
    let recipients = vec![
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    ];
    let amounts = vec![u64::MAX, 100u64];
    let mut store: TemporaryStore<()> = temporary_store::empty_for_testing();
    let mut ctx = TxContext::random_for_testing_only();

    assert_eq!(
        pay(&mut store, coin_objects, recipients, amounts, &mut ctx)
            .unwrap_err()
            .to_execution_status(),
        ExecutionFailureStatus::TotalAmountOverflow
    );
}

#[test]
fn test_pay_insufficient_balance() {
    let coin_objects = vec![
        Object::new_gas_with_balance_and_owner_for_testing(
            10,
            SuiAddress::random_for_testing_only(),
        ),
        Object::new_gas_with_balance_and_owner_for_testing(
            5,
            SuiAddress::random_for_testing_only(),
        ),
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

#[cfg(test)]
fn input_object_kind(object: &Object) -> sui_types::messages::InputObjectKind {
    match &object.owner {
        Owner::Shared {
            initial_shared_version,
            ..
        } => sui_types::messages::InputObjectKind::SharedMoveObject {
            id: object.id(),
            initial_shared_version: *initial_shared_version,
            mutable: true,
        },
        Owner::ObjectOwner(_) | Owner::AddressOwner(_) | Owner::Immutable => {
            sui_types::messages::InputObjectKind::ImmOrOwnedMoveObject(
                object.compute_object_reference(),
            )
        }
    }
}

#[cfg(test)]
/// Test only method that return mutable InputObjects from given objects
fn input_objects_from_objects(objects: Vec<Object>) -> InputObjects {
    InputObjects::new(
        objects
            .into_iter()
            .map(|o| (input_object_kind(&o), o))
            .collect(),
    )
}

#[test]
fn test_pay_success_without_delete() {
    // supplied one coin and only needed to use part of it. should
    // mutate 1 object, create 1 object, and delete no objects
    let sender = SuiAddress::random_for_testing_only();
    let coin1 = Object::new_gas_with_balance_and_owner_for_testing(10, sender);
    let coin2 = Object::new_gas_with_balance_and_owner_for_testing(5, sender);
    let coin_objects = vec![coin1, coin2];
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![6, 3];
    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
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
    let coin1 = Object::new_gas_with_balance_and_owner_for_testing(10, sender);
    let coin2 = Object::new_gas_with_balance_and_owner_for_testing(5, sender);
    let input_coin_id1 = coin1.id();
    let input_coin_id2 = coin2.id();
    let coin_objects = vec![coin1, coin2];
    let recipient = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient];
    let amounts = vec![11];
    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
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
    let coin1 = Object::new_gas_with_balance_and_owner_for_testing(10, sender);
    let coin2 = Object::new_gas_with_balance_and_owner_for_testing(5, sender);
    let input_coin_id1 = coin1.id();
    let input_coin_id2 = coin2.id();
    let coin_objects = vec![coin1, coin2];
    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![4, 11];
    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
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
    let coin = Object::new_gas_with_balance_and_owner_for_testing(18, sender);
    let mut coin_objects = vec![coin];

    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2];
    let amounts = vec![8, 6];

    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
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
    let coin1 = Object::new_gas_with_balance_and_owner_for_testing(30, sender);
    let coin2 = Object::new_gas_with_balance_and_owner_for_testing(20, sender);
    let coin3 = Object::new_gas_with_balance_and_owner_for_testing(10, sender);
    let input_coin_id2 = coin2.id();
    let input_coin_id3 = coin3.id();
    let mut coin_objects = vec![coin1, coin2, coin3];

    let recipient1 = SuiAddress::random_for_testing_only();
    let recipient2 = SuiAddress::random_for_testing_only();
    let recipient3 = SuiAddress::random_for_testing_only();
    let recipients = vec![recipient1, recipient2, recipient3];
    let amounts = vec![5, 15, 25];

    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
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
    let coin1 = Object::new_gas_with_balance_and_owner_for_testing(30, sender);
    let coin2 = Object::new_gas_with_balance_and_owner_for_testing(20, sender);
    let coin3 = Object::new_gas_with_balance_and_owner_for_testing(10, sender);
    let input_coin_id2 = coin2.id();
    let input_coin_id3 = coin3.id();
    let mut coin_objects = vec![coin1, coin2, coin3];

    let recipient = SuiAddress::random_for_testing_only();

    let mut store: TemporaryStore<()> = temporary_store::with_input_objects_for_testing(
        input_objects_from_objects(coin_objects.clone()),
    );
    assert!(pay_all_sui(sender, &mut store, &mut coin_objects, recipient).is_ok());
    let (store, _events) = store.into_inner();

    assert_eq!(store.deleted.len(), 2);
    assert!(store.deleted.contains_key(&input_coin_id2));
    assert!(store.deleted.contains_key(&input_coin_id3));
    assert_eq!(store.written.len(), 1);
    assert!(store.created().is_empty());

    let owner_objs = store.get_written_objects_owned_by(&sender);
    assert!(owner_objs.is_empty());
}
