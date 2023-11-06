// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sandbox::utils::{
        explain_publish_changeset, explain_publish_error, get_gas_status, module,
        on_disk_state_view::OnDiskStateView,
    },
    NativeFunctionRecord,
};
use anyhow::{bail, Result};
use move_binary_format::errors::Location;
use move_command_line_common::env::get_bytecode_version_from_env;
use move_package::compilation::compiled_package::CompiledPackage;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::gas_schedule::CostTable;
use std::collections::BTreeMap;

pub fn publish(
    natives: impl IntoIterator<Item = NativeFunctionRecord>,
    cost_table: &CostTable,
    state: &OnDiskStateView,
    package: &CompiledPackage,
    ignore_breaking_changes: bool,
    with_deps: bool,
    bundle: bool,
    override_ordering: Option<&[String]>,
    verbose: bool,
) -> Result<()> {
    // collect all modules compiled
    let compiled_modules = if with_deps {
        package.all_modules().collect::<Vec<_>>()
    } else {
        package.root_modules().collect::<Vec<_>>()
    };
    if verbose {
        println!("Found {} modules", compiled_modules.len());
    }

    // order the modules for publishing
    let modules_to_publish = match override_ordering {
        Some(ordering) => {
            let module_map: BTreeMap<_, _> = compiled_modules
                .into_iter()
                .map(|unit| (unit.unit.name().to_string(), unit))
                .collect();

            let mut ordered_modules = vec![];
            for name in ordering {
                match module_map.get(name) {
                    None => bail!("Invalid module name in publish ordering: {}", name),
                    Some(unit) => {
                        ordered_modules.push(*unit);
                    }
                }
            }
            ordered_modules
        }
        None => compiled_modules,
    };

    let republished = modules_to_publish
        .iter()
        .filter_map(|unit| {
            let id = module(&unit.unit).ok()?.self_id();
            if state.has_module(&id) {
                Some(format!("{}", id))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if !republished.is_empty() {
        eprintln!(
            "Tried to republish the following modules: {}",
            republished.join(", ")
        );
        return Ok(());
    }

    let bytecode_version = get_bytecode_version_from_env();

    // use the the publish_module API from the VM if we do not allow breaking changes
    if !ignore_breaking_changes {
        let vm = MoveVM::new(natives).unwrap();
        let mut gas_status = get_gas_status(cost_table, None)?;
        let mut session = vm.new_session(state);
        let mut has_error = false;

        if bundle {
            // publish all modules together as a bundle
            let mut sender_opt = None;
            let mut module_bytes_vec = vec![];
            for unit in &modules_to_publish {
                let module_bytes = unit.unit.serialize(bytecode_version);
                module_bytes_vec.push(module_bytes);

                let module_address = *module(&unit.unit)?.self_id().address();
                match &sender_opt {
                    None => {
                        sender_opt = Some(module_address);
                    }
                    Some(val) => {
                        if val != &module_address {
                            bail!("All modules in the bundle must share the same address");
                        }
                    }
                }
            }
            match sender_opt {
                None => bail!("No modules to publish"),
                Some(sender) => {
                    let res =
                        session.publish_module_bundle(module_bytes_vec, sender, &mut gas_status);
                    if let Err(err) = res {
                        println!("Invalid multi-module publishing: {}", err);
                        if let Location::Module(module_id) = err.location() {
                            // find the module where error occures and explain
                            if let Some(unit) = modules_to_publish
                                .into_iter()
                                .find(|&x| x.unit.name().as_str() == module_id.name().as_str())
                            {
                                explain_publish_error(err, state, unit)?
                            } else {
                                println!("Unable to locate the module in the multi-module publishing error");
                            }
                        }
                        has_error = true;
                    }
                }
            }
        } else {
            // publish modules sequentially, one module at a time
            for unit in &modules_to_publish {
                let module_bytes = unit.unit.serialize(bytecode_version);
                let id = module(&unit.unit)?.self_id();
                let sender = *id.address();

                let res = session.publish_module(module_bytes, sender, &mut gas_status);
                if let Err(err) = res {
                    explain_publish_error(err, state, unit)?;
                    has_error = true;
                    break;
                }
            }
        }

        if !has_error {
            let (changeset, events) = session.finish().0.map_err(|e| e.into_vm_status())?;
            assert!(events.is_empty());
            if verbose {
                explain_publish_changeset(&changeset);
            }
            let modules: Vec<_> = changeset
                .into_modules()
                .map(|(module_id, blob_opt)| {
                    (module_id, blob_opt.ok().expect("must be non-deletion"))
                })
                .collect();
            state.save_modules(&modules)?;
        }
    } else {
        // NOTE: the VM enforces the most strict way of module republishing and does not allow
        // backward incompatible changes, as as result, if this flag is set, we skip the VM process
        // and force the CLI to override the on-disk state directly
        let mut serialized_modules = vec![];
        for unit in modules_to_publish {
            let id = module(&unit.unit)?.self_id();
            let module_bytes = unit.unit.serialize(bytecode_version);
            serialized_modules.push((id, module_bytes));
        }
        state.save_modules(&serialized_modules)?;
    }

    Ok(())
}
