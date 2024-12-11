// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
#![allow(hidden_glob_reexports)]
use crate::sandbox::utils::on_disk_state_view::OnDiskStateView;
use anyhow::{bail, Result};
use move_binary_format::{
    errors::{Location, VMError},
    file_format::{AbilitySet, CompiledModule, SignatureToken},
};
use move_command_line_common::files::MOVE_COMPILED_EXTENSION;
use move_core_types::{
    language_storage::TypeTag,
    transaction_argument::TransactionArgument,
    vm_status::{StatusCode, StatusType},
};
use move_vm_runtime::dev_utils::gas_schedule::Gas;
use move_vm_runtime::dev_utils::gas_schedule::{CostTable, GasStatus};
use std::{fs, path::Path};

pub mod on_disk_state_view;
pub mod package_context;
pub use on_disk_state_view::*;
pub use package_context::*;

pub fn get_gas_status(cost_table: &CostTable, gas_budget: Option<u64>) -> Result<GasStatus> {
    let gas_status = if let Some(gas_budget) = gas_budget {
        // TODO(Gas): This should not be hardcoded.
        let max_gas_budget = u64::MAX.checked_div(1000).unwrap();
        if gas_budget >= max_gas_budget {
            bail!("Gas budget set too high; maximum is {}", max_gas_budget)
        }
        GasStatus::new(cost_table, Gas::new(gas_budget))
    } else {
        // no budget specified. Disable gas metering
        GasStatus::new_unmetered()
    };
    Ok(gas_status)
}

pub(crate) fn explain_type_error(
    script_params: &[SignatureToken],
    txn_args: &[TransactionArgument],
) {
    // TODO: printing type(s) of missing arguments could be useful
    if script_params.len() != txn_args.len() {
        println!(
            "Execution failed with incorrect number of arguments: script expected {:?}, but found \
             {:?}",
            script_params.len(),
            txn_args.len()
        );
        return;
    }

    // TODO: print more helpful error message pinpointing the (argument, type)
    // pair that didn't match
    println!("Execution failed with type error when binding type arguments to type parameters")
}

/// Explain an execution error
pub(crate) fn explain_execution_error(
    error: VMError,
    state: &OnDiskStateView,
    script_type_parameters: &[AbilitySet],
    script_parameters: &[SignatureToken],
    vm_type_args: &[TypeTag],
    txn_args: &[TransactionArgument],
) -> Result<()> {
    use StatusCode::*;
    match (error.location(), error.major_status(), error.sub_status()) {
        (Location::Module(module_id), StatusCode::ABORTED, Some(abort_code)) => {
            println!(
                "Execution aborted with code {} in module {}.",
                abort_code, module_id
            );
        }
        (location, status_code, _) if error.status_type() == StatusType::Execution => {
            let (function, code_offset) = error.offsets()[0];
            let status_explanation = match status_code {
                RESOURCE_ALREADY_EXISTS => {
                    "a RESOURCE_ALREADY_EXISTS error (i.e., \
                    `move_to<T>(account)` when there is already a \
                    resource of type `T` under `account`)"
                }
                MISSING_DATA => {
                    "a RESOURCE_DOES_NOT_EXIST error (i.e., `move_from<T>(a)`, \
                    `borrow_global<T>(a)`, or `borrow_global_mut<T>(a)` when there \
                    is no resource of type `T` at address `a`)"
                }
                ARITHMETIC_ERROR => {
                    "an arithmetic error (i.e., integer overflow/underflow, \
                        div/mod by zero, or invalid shift)"
                }
                VECTOR_OPERATION_ERROR => {
                    "an error originated from vector operations (i.e., \
                        index out of bound, pop an empty vector, or unpack a \
                        vector with a wrong parity)"
                }
                EXECUTION_STACK_OVERFLOW => "an execution stack overflow",
                CALL_STACK_OVERFLOW => "a call stack overflow",
                OUT_OF_GAS => "an out of gas error",
                _ => "an execution error",
            };
            // TODO: map to source code location
            let location_explanation = match location {
                Location::Module(id) => {
                    format!(
                        "{}::{}",
                        id,
                        state
                            .resolve_function(*id.address(), id.name(), function.0)?
                            .unwrap()
                    )
                }
                Location::Undefined => "UNDEFINED".to_owned(),
            };
            println!(
                "Execution failed because of {} in {} at code offset {}",
                status_explanation, location_explanation, code_offset
            )
        }
        (_, NUMBER_OF_TYPE_ARGUMENTS_MISMATCH, _) => println!(
            "Execution failed with incorrect number of type arguments: script expected {:?}, but \
             found {:?}",
            script_type_parameters.len(),
            vm_type_args.len()
        ),
        (_, TYPE_MISMATCH, _) => explain_type_error(script_parameters, txn_args),
        (_, LINKER_ERROR, _) => {
            // TODO: is this the only reason we can see LINKER_ERROR?
            // Can we also see it if someone manually deletes modules in storage?
            println!(
                "Execution failed due to unresolved type argument(s) (i.e., `--type-args \
                 0x1::M:T` when there is no module named M at 0x1 or no type named T in module \
                 0x1::M)"
            );
        }
        (_, status_code, _) => {
            println!("Execution failed with unexpected error {:?}", status_code)
        }
    }
    Ok(())
}

/// Return `true` if `path` is a Move bytecode file based on its extension
pub(crate) fn is_bytecode_file(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == MOVE_COMPILED_EXTENSION)
}

/// Return `true` if path contains a valid Move bytecode module
pub(crate) fn contains_module(path: &Path) -> bool {
    is_bytecode_file(path)
        && match fs::read(path) {
            Ok(bytes) => CompiledModule::deserialize_with_defaults(&bytes).is_ok(),
            Err(_) => false,
        }
}
