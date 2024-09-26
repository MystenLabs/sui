// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sandbox::utils::{
        explain_publish_changeset, explain_publish_error, get_gas_status,
        on_disk_state_view::OnDiskStateView,
    },
    NativeFunctionRecord,
};
use anyhow::{bail, Result};
use move_binary_format::errors::Location;
use move_bytecode_utils::dependency_graph::DependencyGraph;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_core_types::account_address::AccountAddress;
use move_package::compilation::compiled_package::CompiledPackage;
use move_vm_runtime::{
    cache::linkage_context::LinkageContext, natives::functions::NativeFunctions,
    test_utils::gas_schedule::CostTable, vm::vm::VirtualMachine,
};
use std::collections::{BTreeMap, HashMap};

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
    // TODO: What about ordering?
    // let modules_to_publish = match override_ordering {
    //     Some(ordering) => {
    //         let module_map: BTreeMap<_, _> = compiled_modules
    //             .into_iter()
    //             .map(|unit| (unit.unit.name().to_string(), unit))
    //             .collect();

    //         let mut ordered_modules = vec![];
    //         for name in ordering {
    //             match module_map.get(name) {
    //                 None => bail!("Invalid module name in publish ordering: {}", name),
    //                 Some(unit) => {
    //                     ordered_modules.push(*unit);
    //                 }
    //             }
    //         }
    //         ordered_modules
    //     }
    //     None => compiled_modules,
    // };

    let republished = compiled_modules
        .iter()
        .filter_map(|unit| {
            let id = unit.unit.module.self_id();
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

    let compiled_modules = compiled_modules
        .into_iter()
        .map(|module| &module.unit)
        .collect::<Vec<_>>();

    // Build the dependency graph from the `CompiledModule`s
    let dependency_graph = DependencyGraph::new(
        compiled_modules
            .iter()
            .map(|module| &module.module)
            .collect::<Vec<_>>(),
    );

    let packages_to_publish = sort_modules_into_packages(compiled_modules, &dependency_graph);

    // use the publish_module API from the VM if we do not allow breaking changes
    if !ignore_breaking_changes {
        let natives = NativeFunctions::new(natives)?;
        let mut vm = VirtualMachine::new_with_default_config(natives);

        let mut gas_status = get_gas_status(cost_table, None)?;

        for (package_id, modules) in packages_to_publish {
            // Retrieve the dependency set for the current package
            let dependencies = dependency_graph
                .find_all_dependencies(&package_id)
                .expect("Failed to get dependencies");

            // Create a dependency map where each AccountAddress maps to itself
            let mut dependency_map: HashMap<AccountAddress, AccountAddress> = HashMap::new();
            for dep in dependencies {
                dependency_map.insert(dep, dep); // Each value maps to itself
            }
            dependency_map.insert(package_id, package_id);

            // Create a `LinkageContext`
            let linkage_context = LinkageContext::new(package_id, dependency_map);

            // Serialize the modules to prepare them for publishing
            let serialized_modules: Vec<Vec<u8>> =
                modules.iter().map(|module| module.serialize()).collect();

            // Publish the package using the VM
            let (publish_result, _) = vm.publish_package(
                state,
                &linkage_context,
                package_id,
                package_id,
                serialized_modules,
                &mut gas_status,
            );
            let changeset = publish_result?;
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

        // FIXME: Unbundled publication doesn't even make sense for the VM rewrite
        // if bundle {
        //     // publish all modules together as a bundle
        //     let mut sender_opt = None;
        //     let mut module_bytes_vec = vec![];
        //     for unit in &modules_to_publish {
        //         let module_bytes = unit.unit.serialize();
        //         module_bytes_vec.push(module_bytes);

        //         let module_address = *unit.unit.module.self_id().address();
        //         match &sender_opt {
        //             None => {
        //                 sender_opt = Some(module_address);
        //             }
        //             Some(val) => {
        //                 if val != &module_address {
        //                     bail!("All modules in the bundle must share the same address");
        //                 }
        //             }
        //         }
        //     }
        //     match sender_opt {
        //         None => bail!("No modules to publish"),
        //         Some(sender) => {
        //             let res =
        //                 session.publish_module_bundle(module_bytes_vec, sender, &mut gas_status);
        //             if let Err(err) = res {
        //                 println!("Invalid multi-module publishing: {}", err);
        //                 if let Location::Module(module_id) = err.location() {
        //                     // find the module where error occures and explain
        //                     if let Some(unit) = modules_to_publish
        //                         .into_iter()
        //                         .find(|&x| x.unit.name().as_str() == module_id.name().as_str())
        //                     {
        //                         explain_publish_error(err, state, unit)?
        //                     } else {
        //                         println!("Unable to locate the module in the multi-module publishing error");
        //                     }
        //                 }
        //                 has_error = true;
        //             }
        //         }
        //     }
        // } else {
        //     // publish modules sequentially, one module at a time
        //     for unit in &modules_to_publish {
        //         let module_bytes = unit.unit.serialize();
        //         let id = unit.unit.module.self_id();
        //         let sender = *id.address();

        //         let res = session.publish_module(module_bytes, sender, &mut gas_status);
        //         if let Err(err) = res {
        //             explain_publish_error(err, state, unit)?;
        //             has_error = true;
        //             break;
        //         }
        //     }
        // }
    } else {
        // NOTE: the VM enforces the most strict way of module republishing and does not allow
        // backward incompatible changes, as as result, if this flag is set, we skip the VM process
        // and force the CLI to override the on-disk state directly
        let mut serialized_modules = vec![];
        for (_package, modules) in packages_to_publish {
            for module in modules {
                let id = module.module.self_id();
                let module_bytes = module.serialize();
                serialized_modules.push((id, module_bytes));
            }
        }
        state.save_modules(&serialized_modules)?;
    }

    Ok(())
}

fn sort_modules_into_packages<'a>(
    modules: Vec<&'a NamedCompiledModule>,
    dependency_graph: &DependencyGraph,
) -> Vec<(AccountAddress, Vec<&'a NamedCompiledModule>)> {
    // Compute the topological order of the modules.
    let sorted_modules = dependency_graph
        .compute_topological_order()
        .expect("Circular dependency detected");

    // Create a mapping of `AccountAddress` to the list of corresponding `NamedCompiledModule`s.
    let mut address_to_modules: BTreeMap<AccountAddress, Vec<&NamedCompiledModule>> =
        BTreeMap::new();

    for module in sorted_modules {
        // Find the corresponding `NamedCompiledModule` from the original `modules` vector.
        if let Some(named_module) = modules.iter().find(|m| &m.module == module) {
            address_to_modules
                .entry(named_module.address.into_inner())
                .or_default()
                .push(named_module);
        }
    }

    // Convert the map into a sorted vector of `(AccountAddress, Vec<NamedCompiledModule>)`.
    address_to_modules.into_iter().collect()
}
