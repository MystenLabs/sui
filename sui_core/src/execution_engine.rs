// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::authority::AuthorityTemporaryStore;
use move_vm_runtime::native_functions::NativeFunctionTable;
use sui_adapter::adapter;
use sui_types::{
    base_types::{SuiAddress, TransactionDigest, TxContext},
    error::SuiResult,
    gas::{self, SuiGasStatus},
    messages::{
        ExecutionStatus, InputObjectKind, SingleTransactionKind, Transaction, TransactionEffects,
    },
    object::Object,
    storage::{BackingPackageStore, Storage},
};
use tracing::{debug, instrument};

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<S: BackingPackageStore>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    transaction: Transaction,
    transaction_digest: TransactionDigest,
    objects_by_kind: Vec<(InputObjectKind, Object)>,
    move_vm: &Arc<adapter::SuiMoveVM>,
    native_functions: &NativeFunctionTable,
    gas_status: SuiGasStatus,
) -> SuiResult<TransactionEffects> {
    let mut transaction_dependencies: BTreeSet<_> = objects_by_kind
        .iter()
        .map(|(_, object)| object.previous_transaction)
        .collect();

    let mut tx_ctx = TxContext::new(&transaction.sender_address(), &transaction_digest);

    let gas_object_id = transaction.gas_payment_object_ref().0;
    let status = execute_transaction(
        temporary_store,
        transaction,
        objects_by_kind,
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
    transaction: Transaction,
    mut objects_by_kind: Vec<(InputObjectKind, Object)>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<adapter::SuiMoveVM>,
    native_functions: &NativeFunctionTable,
    mut gas_status: SuiGasStatus,
) -> ExecutionStatus {
    // unwraps here are safe because we built `inputs`
    let mut gas_object = objects_by_kind.pop().unwrap().1;
    let mut object_input_iter = objects_by_kind.into_iter().map(|(_, object)| object);
    let mut result = Ok(vec![]);
    // TODO: Since we require all mutable objects to not show up more than
    // once across single tx, we should be able to run them in parallel.
    for single_tx in transaction.into_single_transactions() {
        let input_size = single_tx.input_object_count();
        match single_tx {
            SingleTransactionKind::Transfer(t) => {
                let inputs = object_input_iter.by_ref().take(input_size).collect();
                if let Err(err) = transfer(temporary_store, inputs, t.recipient) {
                    result = Err(err);
                    break;
                }
            }
            SingleTransactionKind::Call(c) => {
                let mut inputs: Vec<_> = object_input_iter.by_ref().take(input_size).collect();
                // unwraps here are safe because we built `inputs`
                let package = inputs.pop().unwrap();
                result = adapter::execute(
                    move_vm,
                    temporary_store,
                    native_functions,
                    &package,
                    &c.module,
                    &c.function,
                    c.type_arguments.clone(),
                    inputs,
                    c.pure_arguments.clone(),
                    &mut gas_status,
                    tx_ctx,
                );
                if result.is_err() {
                    break;
                }
            }
            SingleTransactionKind::Publish(m) => {
                if let Err(err) = adapter::publish(
                    temporary_store,
                    native_functions.clone(),
                    m.modules,
                    tx_ctx,
                    &mut gas_status,
                ) {
                    result = Err(err);
                    break;
                }
            }
        };
    }
    if result.is_err() {
        // Roll back the temporary store if execution failed.
        temporary_store.reset();
    }
    temporary_store.ensure_active_inputs_mutated();
    if let Err(err) = temporary_store
        .charge_gas_for_storage_changes(&mut gas_status, gas_object.object_size_for_gas_metering())
    {
        result = Err(err);
    }

    let cost_summary = gas_status.summary(result.is_ok());
    let gas_used = cost_summary.gas_used();
    gas::deduct_gas(&mut gas_object, gas_used);
    temporary_store.write_object(gas_object);

    // TODO: Return cost_summary so that the detailed summary exists in TransactionEffects for
    // gas and rebate distribution.
    match result {
        Ok(results) => ExecutionStatus::Success {
            gas_cost: cost_summary,
            results,
        },
        Err(error) => ExecutionStatus::new_failure(cost_summary, error),
    }
}

fn transfer<S>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut inputs: Vec<Object>,
    recipient: SuiAddress,
) -> SuiResult {
    // Safe to unwrap since we constructed the inputs.
    let mut output_object = inputs.pop().unwrap();
    output_object.transfer(recipient)?;
    temporary_store.write_object(output_object);
    Ok(())
}
