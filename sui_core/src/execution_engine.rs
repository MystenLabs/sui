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
    messages::{ExecutionStatus, Transaction, TransactionKind},
    object::Object,
    storage::{BackingPackageStore, Storage},
};

pub fn execute_transaction<S: BackingPackageStore>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    transaction: Transaction,
    mut inputs: Vec<Object>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<adapter::MoveVM>,
    native_functions: NativeFunctionTable,
) -> SuiResult<ExecutionStatus> {
    // unwraps here are safe because we built `inputs`
    let mut gas_object = inputs.pop().unwrap();

    let status = match transaction.data.kind {
        TransactionKind::Transfer(t) => {
            transfer(temporary_store, inputs, t.recipient, gas_object.clone())
        }
        TransactionKind::Call(c) => {
            // unwraps here are safe because we built `inputs`
            let package = inputs.pop().unwrap();
            adapter::execute(
                move_vm,
                temporary_store,
                native_functions,
                package,
                &c.module,
                &c.function,
                c.type_arguments,
                inputs,
                c.pure_arguments,
                c.gas_budget,
                gas_object.clone(),
                tx_ctx,
            )
        }
        TransactionKind::Publish(m) => adapter::publish(
            temporary_store,
            native_functions,
            m.modules,
            tx_ctx,
            m.gas_budget,
            gas_object.clone(),
        ),
    }?;
    if let ExecutionStatus::Failure { gas_used, .. } = &status {
        // Roll back the temporary store if execution failed.
        temporary_store.reset();
        // This gas deduction cannot fail.
        gas::deduct_gas(&mut gas_object, *gas_used);
        temporary_store.write_object(gas_object);
    }
    temporary_store.ensure_active_inputs_mutated();
    Ok(status)
}

fn transfer<S>(
    temporary_store: &mut AuthorityTemporaryStore<S>,
    mut inputs: Vec<Object>,
    recipient: SuiAddress,
    mut gas_object: Object,
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
    if let Err(err) = gas::try_deduct_gas(&mut gas_object, gas_used) {
        return Ok(ExecutionStatus::Failure {
            gas_used: gas::MIN_OBJ_TRANSFER_GAS,
            error: Box::new(err),
        });
    }
    temporary_store.write_object(gas_object);

    if let Err(err) = output_object.transfer(recipient) {
        return Ok(ExecutionStatus::Failure {
            gas_used: gas::MIN_OBJ_TRANSFER_GAS,
            error: Box::new(err),
        });
    }
    temporary_store.write_object(output_object);
    Ok(ExecutionStatus::Success { gas_used })
}
