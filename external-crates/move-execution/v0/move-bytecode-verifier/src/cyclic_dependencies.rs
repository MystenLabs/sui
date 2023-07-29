// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains verification of usage of dependencies for modules
use move_binary_format::{
    access::ModuleAccess,
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::CompiledModule,
};
use move_core_types::{language_storage::ModuleId, vm_status::StatusCode};
use std::collections::BTreeSet;

pub fn verify_module<D>(module: &CompiledModule, imm_deps: D) -> VMResult<()>
where
    D: Fn(&ModuleId) -> PartialVMResult<Vec<ModuleId>>,
{
    verify_module_impl(module, imm_deps).map_err(|e| e.finish(Location::Module(module.self_id())))
}

/// This function performs a depth-first traversal in the module graph, starting at `module` and
/// recursively exploring immediate dependencies.  During the DFS,
/// - If `module.self_id()` is encountered (again), a dependency cycle is detected and an error is
///   returned.
/// - Otherwise terminates without an error.
fn verify_module_impl<D>(module: &CompiledModule, imm_deps: D) -> PartialVMResult<()>
where
    D: Fn(&ModuleId) -> PartialVMResult<Vec<ModuleId>>,
{
    fn detect_cycles<D>(
        target: &ModuleId,
        cursor: &ModuleId,
        visited: &mut BTreeSet<ModuleId>,
        deps: &D,
    ) -> PartialVMResult<bool>
    where
        D: Fn(&ModuleId) -> PartialVMResult<Vec<ModuleId>>,
    {
        if cursor == target {
            return Ok(true);
        }

        if !visited.insert(cursor.clone()) {
            for dep in deps(cursor)? {
                if detect_cycles(target, &dep, visited, deps)? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    let self_id = module.self_id();
    let mut visited = BTreeSet::new();
    for dep in module.immediate_dependencies() {
        if detect_cycles(&self_id, &dep, &mut visited, &imm_deps)? {
            return Err(PartialVMError::new(StatusCode::CYCLIC_MODULE_DEPENDENCY));
        }
    }

    Ok(())
}
