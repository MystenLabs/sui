// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use petgraph::graphmap::DiGraphMap;

use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Directed graph capturing dependencies between modules
pub struct DependencyGraph<'a> {
    /// Set of modules guaranteed to be closed under dependencies
    modules: Vec<&'a CompiledModule>,
    graph: DiGraphMap<ModuleIndex, ()>,
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, PartialOrd, Ord)]
struct ModuleIndex(usize);

impl<'a> DependencyGraph<'a> {
    /// Construct a dependency graph from a set of `modules`.
    /// Panics if `modules` contains duplicates or is not closed under the depedency relation
    pub fn new(module_iter: impl IntoIterator<Item = &'a CompiledModule>) -> Self {
        let mut modules = vec![];
        let mut reverse_modules = BTreeMap::new();
        for (i, m) in module_iter.into_iter().enumerate() {
            modules.push(m);
            assert!(
                reverse_modules
                    .insert(m.self_id(), ModuleIndex(i))
                    .is_none(),
                "Duplicate module found"
            );
        }
        let mut graph = DiGraphMap::new();
        for module in &modules {
            let module_idx: ModuleIndex = *reverse_modules.get(&module.self_id()).unwrap();
            let deps = module.immediate_dependencies();
            if deps.is_empty() {
                graph.add_node(module_idx);
            } else {
                for dep in deps {
                    let dep_idx = *reverse_modules
                        .get(&dep)
                        .unwrap_or_else(|| panic!("Missing dependency {}", dep));
                    graph.add_edge(dep_idx, module_idx, ());
                }
            }
        }
        DependencyGraph { modules, graph }
    }

    /// Return an iterator over the modules in `self` in topological order--modules with least deps first.
    /// Fails with an error if `self` contains circular dependencies
    pub fn compute_topological_order(&self) -> Result<impl Iterator<Item = &CompiledModule>> {
        match petgraph::algo::toposort(&self.graph, None) {
            Err(_) => bail!("Circular dependency detected"),
            Ok(ordered_idxs) => Ok(ordered_idxs.into_iter().map(move |idx| self.modules[idx.0])),
        }
    }

    /// Given an `AccountAddress`, find all `AccountAddress`es that it depends on (not including itself).
    pub fn find_all_dependencies(
        &self,
        target_address: &AccountAddress,
    ) -> Result<BTreeSet<AccountAddress>> {
        // First, find the `ModuleIndex` corresponding to the target `AccountAddress`
        let target_module_idx = self
            .modules
            .iter()
            .position(|module| module.self_id().address() == target_address)
            .map(ModuleIndex)
            .ok_or_else(|| anyhow!("Target address not found in the graph"))?;

        // Perform a reverse graph traversal (from the target) to find all its dependencies
        let mut dependencies = BTreeSet::new();
        let mut stack = vec![target_module_idx];
        let mut visited = BTreeSet::new();

        while let Some(module_idx) = stack.pop() {
            // Skip if this module was already visited
            if !visited.insert(module_idx) {
                continue;
            }

            // For each incoming edge (dependency), add the address and push to the stack
            for neighbor in self.graph.neighbors_directed(module_idx, petgraph::Direction::Incoming) {
                let dependency_address = self.modules[neighbor.0].address();
                if dependency_address != target_address {
                    dependencies.insert(*dependency_address);
                    stack.push(neighbor);
                }
            }
        }
        Ok(dependencies)
    }
}
