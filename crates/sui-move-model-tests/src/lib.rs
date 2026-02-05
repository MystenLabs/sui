// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for move-model-2 with Sui system packages.
//!
//! This crate tests move-model-2's call graph functionality against the
//! Sui framework system packages (move-stdlib, sui-framework, sui-system, etc.),
//! which represent the most commonly used Move code on mainnet.

#[cfg(test)]
mod tests {
    use move_binary_format::CompiledModule;
    use move_core_types::account_address::AccountAddress;
    use move_model_2::{
        call_graph::{CallGraph, TopologicalItem},
        compiled_model::Model,
        model::ModelConfig,
    };
    use move_symbol_pool::Symbol;
    use std::collections::BTreeMap;
    use sui_framework::BuiltInFramework;
    use sui_types::{
        BRIDGE_PACKAGE_ID, DEEPBOOK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID,
        SUI_SYSTEM_PACKAGE_ID,
    };

    /// Load all modules from the system packages.
    fn load_all_system_modules() -> Vec<CompiledModule> {
        BuiltInFramework::iter_system_packages()
            .flat_map(|pkg| pkg.modules())
            .collect()
    }

    /// Build named address reverse map for the system packages.
    fn build_address_map() -> BTreeMap<AccountAddress, Symbol> {
        let mut map = BTreeMap::new();
        map.insert(*MOVE_STDLIB_PACKAGE_ID, Symbol::from("std"));
        map.insert(*SUI_FRAMEWORK_PACKAGE_ID, Symbol::from("sui"));
        map.insert(*SUI_SYSTEM_PACKAGE_ID, Symbol::from("sui_system"));
        map.insert(*DEEPBOOK_PACKAGE_ID, Symbol::from("deepbook"));
        map.insert(*BRIDGE_PACKAGE_ID, Symbol::from("bridge"));
        map
    }

    #[test]
    fn test_call_graph_construction_all_system_packages() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        // Basic sanity checks
        let function_count = call_graph.function_count();
        let edge_count = call_graph.edge_count();

        println!("System packages call graph stats:");
        println!("  Functions: {}", function_count);
        println!("  Call edges: {}", edge_count);

        assert!(
            function_count > 100,
            "Expected many functions in system packages, got {}",
            function_count
        );
        assert!(
            edge_count > 50,
            "Expected many call edges, got {}",
            edge_count
        );
    }

    #[test]
    fn test_topological_order_all_system_packages() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        let mut single_count = 0;
        let mut recursive_count = 0;
        let mut recursive_function_count = 0;

        for item in call_graph.topological_order() {
            match item {
                TopologicalItem::Single(_) => single_count += 1,
                TopologicalItem::Recursive(scc) => {
                    recursive_count += 1;
                    recursive_function_count += scc.functions.len();
                }
            }
        }

        println!("Topological order stats:");
        println!("  Non-recursive functions: {}", single_count);
        println!("  Recursive SCCs: {}", recursive_count);
        println!("  Functions in recursive SCCs: {}", recursive_function_count);

        // All functions should be accounted for
        assert_eq!(
            single_count + recursive_function_count,
            call_graph.function_count()
        );
    }

    #[test]
    fn test_callees_exist_in_graph() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        // For each function, verify that all its callees exist in the graph
        for func in call_graph.functions() {
            if let Some(callees) = call_graph.callees(func) {
                for callee in callees {
                    assert!(
                        call_graph.contains(&callee),
                        "Callee {:?} called by {:?} not found in graph",
                        callee,
                        func
                    );
                }
            }
        }
    }

    #[test]
    fn test_callers_exist_in_graph() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        // For each function, verify that all its callers exist in the graph
        for func in call_graph.functions() {
            if let Some(callers) = call_graph.callers(func) {
                for caller in callers {
                    assert!(
                        call_graph.contains(&caller),
                        "Caller {:?} of {:?} not found in graph",
                        caller,
                        func
                    );
                }
            }
        }
    }

    #[test]
    fn test_sccs_partition_functions() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        // Collect all functions from SCCs
        let mut functions_from_sccs = std::collections::BTreeSet::new();
        for scc in call_graph.sccs() {
            for func in scc.functions {
                let is_new = functions_from_sccs.insert(func);
                assert!(is_new, "Function {:?} appears in multiple SCCs", func);
            }
        }

        // Should match all functions in the graph
        let all_functions: std::collections::BTreeSet<_> =
            call_graph.functions().copied().collect();
        assert_eq!(
            functions_from_sccs, all_functions,
            "SCCs should partition all functions"
        );
    }

    #[test]
    fn test_transitive_closure_consistency() {
        let modules = load_all_system_modules();
        let address_map = build_address_map();

        let model = Model::from_compiled(&address_map, modules);
        let call_graph = CallGraph::from_model(&model);

        // For a sample of functions, verify transitive closure consistency
        let sample: Vec<_> = call_graph.functions().take(20).copied().collect();

        for func in sample {
            // If A calls B transitively, then B should be in transitive_callees(A)
            let transitive_callees: std::collections::BTreeSet<_> =
                call_graph.transitive_callees(&func).collect();

            // Direct callees should be subset of transitive callees
            if let Some(direct_callees) = call_graph.callees(&func) {
                for direct in direct_callees {
                    assert!(
                        transitive_callees.contains(&direct),
                        "Direct callee {:?} not in transitive callees of {:?}",
                        direct,
                        func
                    );
                }
            }
        }
    }

    #[test]
    fn test_individual_packages() {
        let address_map = build_address_map();
        let config = ModelConfig {
            allow_missing_dependencies: true,
        };

        for pkg in BuiltInFramework::iter_system_packages() {
            let modules = pkg.modules();
            let model = Model::from_compiled_with_config(config.clone(), &address_map, modules);
            let call_graph = CallGraph::from_model(&model);

            println!(
                "Package {:?}: {} functions, {} edges",
                pkg.id,
                call_graph.function_count(),
                call_graph.edge_count()
            );

            // Each package should have at least some functions
            assert!(
                call_graph.function_count() > 0,
                "Package {:?} has no functions",
                pkg.id
            );

            // Topological order should cover all functions
            let topo_count: usize = call_graph
                .topological_order()
                .map(|item| match item {
                    TopologicalItem::Single(_) => 1,
                    TopologicalItem::Recursive(scc) => scc.functions.len(),
                })
                .sum();

            assert_eq!(
                topo_count,
                call_graph.function_count(),
                "Topological order doesn't cover all functions in package {:?}",
                pkg.id
            );
        }
    }
}
