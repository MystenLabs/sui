// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::authority::AuthorityTemporaryStore;
use move_vm_runtime::native_functions::NativeFunctionTable;
use sui_adapter::adapter;
use sui_types::{
    base_types::{SuiAddress, TxContext},
    error::{SuiError, SuiResult},
    gas,
    messages::{ExecutionStatus, InputObjectKind, SingleTransactionKind, Transaction},
    object::Object,
    storage::{BackingPackageStore, Storage},
};

pub fn execute_transaction<S: BackingPackageStore>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    transaction: Transaction,
    mut objects_by_kind: Vec<(InputObjectKind, Object)>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<adapter::MoveVM>,
    native_functions: NativeFunctionTable,
) -> SuiResult<ExecutionStatus> {
    // unwraps here are safe because we built `inputs`
    let mut gas_object = objects_by_kind.pop().unwrap().1;
    let mut total_gas = 0;
    // TODO: We only keep the last result for now.
    // We should make the results a vector of results.
    let mut last_results = vec![];
    // TODO: Since we require all mutable objects to not show up more than
    // once across single tx, we should be able to run them in parallel.
    let mut object_input_iter = objects_by_kind.into_iter().map(|(_, object)| object);
    for single_tx in transaction.into_single_transactions() {
        let input_size = single_tx.input_object_count();
        let status = match single_tx {
            SingleTransactionKind::Transfer(t) => {
                let inputs = object_input_iter.by_ref().take(input_size).collect();
                transfer(temporary_store, inputs, t.recipient)
            }
            SingleTransactionKind::Call(c) => {
                let mut inputs: Vec<_> = object_input_iter.by_ref().take(input_size).collect();
                // unwraps here are safe because we built `inputs`
                let package = inputs.pop().unwrap();
                adapter::execute(
                    move_vm,
                    temporary_store,
                    native_functions.clone(),
                    package,
                    &c.module,
                    &c.function,
                    c.type_arguments.clone(),
                    inputs,
                    c.pure_arguments.clone(),
                    c.gas_budget,
                    tx_ctx,
                )
            }
            SingleTransactionKind::Publish(m) => adapter::publish(
                temporary_store,
                native_functions.clone(),
                m.modules,
                tx_ctx,
                m.gas_budget,
            ),
        }?;
        match status {
            ExecutionStatus::Failure { gas_used, error } => {
                // Roll back the temporary store if execution failed.
                temporary_store.reset();
                temporary_store.ensure_active_inputs_mutated();
                total_gas += gas_used;
                return Ok(ExecutionStatus::new_failure(total_gas, *error));
            }
            ExecutionStatus::Success { gas_used, results } => {
                last_results = results;
                total_gas += gas_used;
            }
        }
    }
    gas::deduct_gas(&mut gas_object, total_gas);
    temporary_store.write_object(gas_object);

    temporary_store.ensure_active_inputs_mutated();
    Ok(ExecutionStatus::Success {
        gas_used: total_gas,
        results: last_results,
    })
}

fn transfer<S>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut inputs: Vec<Object>,
    recipient: SuiAddress,
) -> SuiResult<ExecutionStatus> {
    if !inputs.len() == 1 {
        return Ok(ExecutionStatus::Failure {
            gas_used: gas::MIN_OBJ_TRANSFER_GAS,
            error: Box::new(SuiError::ObjectInputArityViolation),
        });
    }

    // Safe to do pop due to check !is_empty()
    let mut output_object = inputs.pop().unwrap();

    let gas_used = gas::calculate_object_transfer_cost(&output_object);

    if let Err(err) = output_object.transfer(recipient) {
        return Ok(ExecutionStatus::Failure {
            gas_used: gas::MIN_OBJ_TRANSFER_GAS,
            error: Box::new(err),
        });
    }
    temporary_store.write_object(output_object);
    Ok(ExecutionStatus::Success {
        gas_used,
        results: vec![],
    })
}
