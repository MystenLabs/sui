// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::Identifier;
use std::{collections::BTreeSet, sync::Arc};

use crate::authority::AuthorityTemporaryStore;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use sui_adapter::adapter;
use sui_types::committee::EpochId;
use sui_types::error::ExecutionError;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::ObjectArg;
use sui_types::object::{MoveObject, Owner, OBJECT_START_VERSION};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, TxContext},
    event::{Event, TransferType},
    gas::{self, SuiGasStatus},
    messages::{
        CallArg, ChangeEpoch, ExecutionStatus, MoveCall, MoveModulePublish, SingleTransactionKind,
        TransactionData, TransactionEffects, TransferCoin, TransferSui,
    },
    object::Object,
    storage::{BackingPackageStore, Storage},
    sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use tracing::{debug, instrument, trace};

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<S: BackingPackageStore>(
    shared_object_refs: Vec<ObjectRef>,
    temporary_store: &mut AuthorityTemporaryStore<S>,
    transaction_data: TransactionData,
    transaction_digest: TransactionDigest,
    mut transaction_dependencies: BTreeSet<TransactionDigest>,
    move_vm: &Arc<MoveVM>,
    native_functions: &NativeFunctionTable,
    gas_status: SuiGasStatus,
    epoch: EpochId,
) -> (TransactionEffects, Option<ExecutionError>) {
    let mut tx_ctx = TxContext::new(&transaction_data.signer(), &transaction_digest, epoch);

    let gas_object_ref = *transaction_data.gas_payment_object_ref();
    let (gas_cost_summary, execution_result) = execute_transaction(
        temporary_store,
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

    let effects = temporary_store.to_effects(
        shared_object_refs,
        &transaction_digest,
        transaction_dependencies.into_iter().collect(),
        gas_cost_summary,
        status,
        gas_object_ref,
    );
    (effects, execution_error)
}

fn charge_gas_for_object_read<S>(
    temporary_store: &AuthorityTemporaryStore<S>,
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
fn execute_transaction<S: BackingPackageStore>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
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
                SingleTransactionKind::TransferCoin(TransferCoin {
                    recipient,
                    object_ref,
                }) => {
                    // unwrap is is safe because we built the object map from the transactions
                    let object = temporary_store
                        .objects()
                        .get(&object_ref.0)
                        .unwrap()
                        .clone();
                    transfer_coin(temporary_store, object, tx_ctx.sender(), recipient)
                }
                SingleTransactionKind::TransferSui(TransferSui { recipient, amount }) => {
                    let gas_object = temporary_store
                        .objects()
                        .get(&gas_object_id)
                        .expect("We constructed the object map so it should always have the gas object id")
                        .clone();
                    transfer_sui(temporary_store, gas_object, recipient, amount, tx_ctx)
                }
                SingleTransactionKind::Call(MoveCall {
                    package,
                    module,
                    function,
                    type_arguments,
                    arguments,
                }) => {
                    let module_id = ModuleId::new(package.0.into(), module);
                    adapter::execute(
                        move_vm,
                        temporary_store,
                        module_id,
                        &function,
                        type_arguments,
                        arguments,
                        &mut gas_status,
                        tx_ctx,
                    )
                }
                SingleTransactionKind::Publish(MoveModulePublish { modules }) => adapter::publish(
                    temporary_store,
                    native_functions.clone(),
                    modules,
                    tx_ctx,
                    &mut gas_status,
                ),
                SingleTransactionKind::ChangeEpoch(ChangeEpoch {
                    epoch,
                    storage_charge,
                    computation_charge,
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
                            CallArg::Object(ObjectArg::SharedObject(SUI_SYSTEM_STATE_OBJECT_ID)),
                            CallArg::Pure(bcs::to_bytes(&epoch).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&storage_charge).unwrap()),
                            CallArg::Pure(bcs::to_bytes(&computation_charge).unwrap()),
                        ],
                        &mut gas_status,
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
        let gas_rebate = cost_summary.storage_rebate;
        gas::deduct_gas(&mut gas_object, gas_used, gas_rebate);
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");
        temporary_store.write_object(gas_object);
    }

    let cost_summary = gas_status.summary(result.is_ok());
    (cost_summary, result)
}

fn transfer_coin<S>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut object: Object,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> Result<(), ExecutionError> {
    object.transfer_and_increment_version(recipient)?;
    temporary_store.log_event(Event::TransferObject {
        package_id: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
        module: Identifier::from(ident_str!("native")),
        function: Identifier::from(ident_str!("transfer_coin")),
        instigator: sender,
        recipient: Owner::AddressOwner(recipient),
        object_id: object.id(),
        version: object.version(),
        destination_addr: recipient,
        type_: TransferType::Coin,
    });
    temporary_store.write_object(object);
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
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut object: Object,
    recipient: SuiAddress,
    amount: Option<u64>,
    tx_ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    #[cfg(debug_assertions)]
    let version = object.version();

    if let Some(amount) = amount {
        // Deduct the amount from the gas coin and update it.
        let mut gas_coin = GasCoin::try_from(&object)?;
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
            MoveObject::new(
                GasCoin::type_(),
                bcs::to_bytes(&GasCoin::new(
                    tx_ctx.fresh_id(),
                    OBJECT_START_VERSION,
                    amount,
                ))
                .expect("Serializing gas object cannot fail"),
            ),
            Owner::AddressOwner(recipient),
            tx_ctx.digest(),
        );
        temporary_store.write_object(new_object);

        // This is necessary for the temporary store to know this new object is not unwrapped.
        let newly_generated_ids = tx_ctx.recreate_all_ids();
        temporary_store.set_create_object_ids(newly_generated_ids);
    } else {
        // If amount is not specified, we simply transfer the entire coin object.
        // We don't want to increment the version number yet because latter gas charge will do it.
        object.transfer_without_version_change(recipient)?;
    }

    // TODO: Emit a new event type for this.

    #[cfg(debug_assertions)]
    assert_eq!(object.version(), version);

    temporary_store.write_object(object);

    Ok(())
}
