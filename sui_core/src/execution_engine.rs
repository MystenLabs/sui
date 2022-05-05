// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::authority::AuthorityTemporaryStore;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use sui_adapter::adapter;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, TxContext},
    error::SuiResult,
    gas::{self, SuiGasStatus},
    messages::{
        ExecutionStatus, MoveCall, MoveModulePublish, SingleTransactionKind, TransactionData,
        TransactionEffects, Transfer,
    },
    object::Object,
    storage::{BackingPackageStore, Storage},
};
use tracing::{debug, instrument};

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
) -> SuiResult<TransactionEffects> {
    let mut tx_ctx = TxContext::new(&transaction_data.signer(), &transaction_digest);

    let gas_object_id = transaction_data.gas_payment_object_ref().0;
    let status = execute_transaction(
        temporary_store,
        transaction_data,
        gas_object_id,
        &mut tx_ctx,
        move_vm,
        native_functions,
        gas_status,
    );
    let gas_cost_summary = status.gas_cost_summary();
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
        status,
        &gas_object_id,
    );
    Ok(effects)
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
) -> ExecutionStatus {
    let mut gas_object = temporary_store
        .objects()
        .get(&gas_object_id)
        .expect("We constructed the object map so it should always have the gas object id")
        .clone();

    let mut result = Ok(());
    // TODO: Since we require all mutable objects to not show up more than
    // once across single tx, we should be able to run them in parallel.
    for single_tx in transaction_data.kind.into_single_transactions() {
        result = match single_tx {
            SingleTransactionKind::Transfer(Transfer {
                recipient,
                object_ref,
            }) => {
                // unwrap is is safe because we built the object map from the transactions
                let object = temporary_store
                    .objects()
                    .get(&object_ref.0)
                    .unwrap()
                    .clone();
                transfer(temporary_store, object, recipient)
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
        };
        if result.is_err() {
            break;
        }
    }
    if result.is_err() {
        // Roll back the temporary store if execution failed.
        temporary_store.reset();
    }
    // Make sure every mutable object's version number is incremented.
    // This needs to happen before `charge_gas_for_storage_changes` so that it
    // can charge gas for all mutated objects properly.
    temporary_store.ensure_active_inputs_mutated();
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
            temporary_store.ensure_active_inputs_mutated();
            result = Err(err);
        }
    }

    let cost_summary = gas_status.summary(result.is_ok());
    let gas_used = cost_summary.gas_used();
    let gas_rebate = cost_summary.storage_rebate;
    gas::deduct_gas(&mut gas_object, gas_used, gas_rebate);
    temporary_store.write_object(gas_object);

    // TODO: Return cost_summary so that the detailed summary exists in TransactionEffects for
    // gas and rebate distribution.
    match result {
        Ok(()) => ExecutionStatus::Success {
            gas_cost: cost_summary,
        },
        Err(error) => ExecutionStatus::new_failure(cost_summary, error),
    }
}

fn transfer<S>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut object: Object,
    recipient: SuiAddress,
) -> SuiResult {
    object.transfer(recipient)?;
    temporary_store.write_object(object);
    Ok(())
}
