// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
#![allow(hidden_glob_reexports)]
use crate::sandbox::utils::on_disk_state_view::OnDiskStateView;
use anyhow::{bail, Result};

use move_binary_format::{
    access::ModuleAccess,
    compatibility::Compatibility,
    errors::{Location, VMError},
    file_format::{AbilitySet, CompiledModule, FunctionDefinitionIndex, SignatureToken},
    normalized, IndexKind,
};
use move_bytecode_utils::Modules;
use move_command_line_common::files::{FileHash, MOVE_COMPILED_EXTENSION};
use move_compiler::diagnostics::{self, report_diagnostics, Diagnostic, Diagnostics, FileName};
use move_core_types::{
    account_address::AccountAddress,
    effects::{ChangeSet, Op},
    errmap::ErrorMapping,
    language_storage::{ModuleId, TypeTag},
    transaction_argument::TransactionArgument,
    vm_status::{StatusCode, StatusType},
};
use move_ir_types::location::Loc;
use move_package::compilation::compiled_package::CompiledUnitWithSource;
use move_vm_test_utils::gas_schedule::Gas;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

pub mod on_disk_state_view;
pub mod package_context;

use move_bytecode_utils::module_cache::GetModule;
use move_vm_test_utils::gas_schedule::{CostTable, GasStatus};
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

pub(crate) fn explain_publish_changeset(changeset: &ChangeSet) {
    // publish effects should contain no resources
    assert!(changeset.resources().next().is_none());
    // total bytes written across all accounts
    let mut total_bytes_written = 0;
    for (addr, name, blob_op) in changeset.modules() {
        match blob_op {
            Op::New(module_bytes) => {
                let bytes_written = addr.len() + name.len() + module_bytes.len();
                total_bytes_written += bytes_written;
                let module_id = ModuleId::new(addr, name.clone());
                println!(
                    "Publishing a new module {} (wrote {:?} bytes)",
                    module_id, bytes_written
                );
            }
            Op::Modify(module_bytes) => {
                let bytes_written = addr.len() + name.len() + module_bytes.len();
                total_bytes_written += bytes_written;
                let module_id = ModuleId::new(addr, name.clone());
                println!(
                    "Updating an existing module {} (wrote {:?} bytes)",
                    module_id, bytes_written
                );
            }
            Op::Delete => {
                panic!("Deleting a module is not supported")
            }
        }
    }
    println!(
        "Wrote {:?} bytes of module ID's and code",
        total_bytes_written
    )
}

pub(crate) fn explain_type_error(
    script_params: &[SignatureToken],
    signers: &[AccountAddress],
    txn_args: &[TransactionArgument],
) {
    use SignatureToken::*;
    let expected_num_signers = script_params
        .iter()
        .filter(|t| match t {
            Reference(r) => r.is_signer(),
            _ => false,
        })
        .count();
    if expected_num_signers != signers.len() {
        println!(
            "Execution failed with incorrect number of signers: script expected {:?}, but found \
             {:?}",
            expected_num_signers,
            signers.len()
        );
        return;
    }

    // TODO: printing type(s) of missing arguments could be useful
    let expected_num_args = script_params.len() - signers.len();
    if expected_num_args != txn_args.len() {
        println!(
            "Execution failed with incorrect number of arguments: script expected {:?}, but found \
             {:?}",
            expected_num_args,
            txn_args.len()
        );
        return;
    }

    // TODO: print more helpful error message pinpointing the (argument, type)
    // pair that didn't match
    println!("Execution failed with type error when binding type arguments to type parameters")
}

pub(crate) fn explain_publish_error(
    error: VMError,
    state: &OnDiskStateView,
    unit: &CompiledUnitWithSource,
) -> Result<()> {
    use StatusCode::*;
    let mut files = HashMap::new();
    let file_contents = std::fs::read_to_string(&unit.source_path)?;
    let file_hash = FileHash::new(&file_contents);
    files.insert(
        file_hash,
        (
            FileName::from(unit.source_path.to_string_lossy()),
            file_contents,
        ),
    );

    let module = &unit.unit.module;
    let module_id = module.self_id();
    let error_clone = error.clone();
    match error.major_status() {
        DUPLICATE_MODULE_NAME => {
            println!("Module {} exists already.", module_id);
        }
        BACKWARD_INCOMPATIBLE_MODULE_UPDATE => {
            println!("Breaking change detected--publishing aborted. Re-run with --ignore-breaking-changes to publish anyway.");

            let old_module = state.get_module_by_id(&module_id)?.unwrap();
            let old_api = normalized::Module::new(&old_module);
            let new_api = normalized::Module::new(module);

            if (Compatibility {
                check_datatype_and_pub_function_linking: false,
                check_datatype_layout: true,
                check_friend_linking: false,
                check_private_entry_linking: true,
                disallowed_new_abilities: AbilitySet::EMPTY,
                disallow_change_datatype_type_params: false,
                disallow_new_variants: false,
            })
            .check(&old_api, &new_api)
            .is_err()
            {
                // TODO: we could choose to make this more precise by walking the global state and looking for published
                // structs of this type. but probably a bad idea
                println!("Layout API for structs of module {} has changed. Need to do a data migration of published structs", module_id)
            } else if (Compatibility {
                check_datatype_and_pub_function_linking: true,
                check_datatype_layout: false,
                check_friend_linking: false,
                check_private_entry_linking: true,
                disallowed_new_abilities: AbilitySet::EMPTY,
                disallow_change_datatype_type_params: false,
                disallow_new_variants: false,
            })
            .check(&old_api, &new_api)
            .is_err()
            {
                // TODO: this will report false positives if we *are* simultaneously redeploying all dependent modules.
                // but this is not easy to check without walking the global state and looking for everything
                println!("Linking API for structs/functions of module {} has changed. Need to redeploy all dependent modules.", module_id)
            }
        }
        CYCLIC_MODULE_DEPENDENCY => {
            println!(
                "Publishing module {} introduces cyclic dependencies.",
                module_id
            );
            // find all cycles with an iterative DFS
            let all_modules = state.get_all_modules()?;
            let code_cache = Modules::new(&all_modules);

            let mut stack = vec![];
            let mut state = BTreeMap::new();
            state.insert(module_id.clone(), true);
            for dep in module.immediate_dependencies() {
                stack.push((code_cache.get_module(&dep)?, false));
            }

            while let Some((cur, is_exit)) = stack.pop() {
                let cur_id = cur.self_id();
                if is_exit {
                    state.insert(cur_id, false);
                } else {
                    state.insert(cur_id, true);
                    stack.push((cur, true));
                    for next in cur.immediate_dependencies() {
                        if let Some(is_discovered_but_not_finished) = state.get(&next) {
                            if *is_discovered_but_not_finished {
                                let cycle_path: Vec<_> = stack
                                    .iter()
                                    .filter(|(_, is_exit)| *is_exit)
                                    .map(|(m, _)| m.self_id().to_string())
                                    .collect();
                                println!(
                                    "Cycle detected: {} -> {} -> {}",
                                    module_id,
                                    cycle_path.join(" -> "),
                                    module_id,
                                );
                            }
                        } else {
                            stack.push((code_cache.get_module(&next)?, false));
                        }
                    }
                }
            }
            println!("Re-run with --ignore-breaking-changes to publish anyway.")
        }
        MISSING_DEPENDENCY => {
            let err_indices = error_clone.indices();
            let mut diags = Diagnostics::new();
            for (ind_kind, table_ind) in err_indices {
                if let IndexKind::FunctionHandle = ind_kind {
                    let native_function = &(module.function_defs())[*table_ind as usize];
                    let fh = module.function_handle_at(native_function.function);
                    let mh = module.module_handle_at(fh.module);
                    let function_source_map = unit
                        .unit
                        .source_map()
                        .get_function_source_map(FunctionDefinitionIndex(*table_ind));
                    if let Ok(map) = function_source_map {
                        let err_string = format!(
                            "Missing implementation for the native function {}::{}",
                            module.identifier_at(mh.name).as_str(),
                            module.identifier_at(fh.name).as_str()
                        );
                        let diag = Diagnostic::new(
                            diagnostics::codes::Declarations::InvalidFunction,
                            (map.definition_location, err_string),
                            Vec::<(Loc, String)>::new(),
                            Vec::<String>::new(),
                        );
                        diags.add(diag);
                    }
                }
            }
            report_diagnostics(&files, diags)
        }
        status_code => {
            println!("Publishing failed with unexpected error {:?}", status_code)
        }
    }

    Ok(())
}

/// Explain an execution error
pub(crate) fn explain_execution_error(
    error_descriptions: &ErrorMapping,
    error: VMError,
    state: &OnDiskStateView,
    script_type_parameters: &[AbilitySet],
    script_parameters: &[SignatureToken],
    vm_type_args: &[TypeTag],
    signers: &[AccountAddress],
    txn_args: &[TransactionArgument],
) -> Result<()> {
    use StatusCode::*;
    match (error.location(), error.major_status(), error.sub_status()) {
        (Location::Module(module_id), StatusCode::ABORTED, Some(abort_code)) => {
            // try to use move-explain to explain the abort

            print!(
                "Execution aborted with code {} in module {}.",
                abort_code, module_id
            );

            if let Some(error_desc) = error_descriptions.get_explanation(module_id, abort_code) {
                println!(
                    " Abort code details:\nName: {}\nDescription:{}",
                    error_desc.code_name, error_desc.code_description,
                )
            } else {
                println!()
            }
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
                        state.resolve_function(id, function.0)?.unwrap()
                    )
                }
                Location::Script => "script".to_owned(),
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
        (_, TYPE_MISMATCH, _) => explain_type_error(script_parameters, signers, txn_args),
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
