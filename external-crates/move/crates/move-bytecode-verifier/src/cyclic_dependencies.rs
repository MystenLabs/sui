// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains verification of usage of dependencies for modules
use move_binary_format::partial_vm_error;
use move_binary_format::{
    errors::{Location, PartialVMResult, VMResult},
    file_format::CompiledModule,
};
use move_core_types::language_storage::ModuleId;
use std::collections::BTreeSet;

/// Rejects any dependency cycle reachable from `module`. On success, returns the reachable module
/// IDs other than `module`. The module must already be bounds checked, and `imm_deps` must resolve
/// dependencies in a fixed, finite graph for the duration of this call.
pub fn verify_module<D>(module: &CompiledModule, imm_deps: D) -> VMResult<BTreeSet<ModuleId>>
where
    D: Fn(&ModuleId) -> PartialVMResult<Vec<ModuleId>>,
{
    verify_module_impl(module, imm_deps).map_err(|e| e.finish(Location::Module(module.self_id())))
}

fn verify_module_impl<D>(
    module: &CompiledModule,
    imm_deps: D,
) -> PartialVMResult<BTreeSet<ModuleId>>
where
    D: Fn(&ModuleId) -> PartialVMResult<Vec<ModuleId>>,
{
    let self_id = module.self_id();
    let mut visiting = BTreeSet::from([self_id.clone()]);
    let mut visited = BTreeSet::new();
    let mut stack = vec![(self_id.clone(), module.immediate_dependencies().into_iter())];

    while let Some((current, mut deps)) = stack.pop() {
        if let Some(dep) = deps.next() {
            stack.push((current, deps));
            if visiting.contains(&dep) {
                return Err(partial_vm_error!(CYCLIC_MODULE_DEPENDENCY));
            }
            if !visited.contains(&dep) {
                let dependencies = imm_deps(&dep)?;
                visiting.insert(dep.clone());
                stack.push((dep, dependencies.into_iter()));
            }
        } else {
            visiting.remove(&current);
            if current != self_id {
                visited.insert(current);
            }
        }
    }

    Ok(visited)
}
