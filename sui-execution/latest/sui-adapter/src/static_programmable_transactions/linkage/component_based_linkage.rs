// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-component linkage refinement.
//!
//! Runs once immediately after loading (before typing). Partitions the PTB's commands into the
//! weakly-connected components of the any-value data-flow graph: two commands share a component
//! iff a value flows between them (directly or transitively). Within each
//! component the per-call linkages of the member MoveCalls are unified into a single shared
//! `ExecutableLinkage`, which then replaces each member's `LoadedFunction.linkage`.
//!
//! Transaction inputs and the gas coin are graph nodes too: every consumer of a given input (or
//! of the gas coin) joins one component, so the input value carries a single linkage identity as
//! it flows across commands.

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer,
            resolution::{ResolutionTable, VersionConstraint, add_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::{Command, InputArg, Inputs, LoadedFunction, Transaction},
    },
};
use move_binary_format::file_format::Visibility;
use petgraph::unionfind::UnionFind;
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionErrorTrait, command_argument_error},
    execution_status::CommandArgumentError,
    transaction::Argument,
};

/// Refine each MoveCall's per-call linkage into a per-component linkage shared across all
/// MoveCalls in its component.
pub fn refine_per_component_linkage<E: ExecutionErrorTrait>(
    txn: &mut Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &dyn PackageStore,
) -> Result<(), E> {
    let n = txn.commands.len();
    if n == 0 {
        return Ok(());
    }
    // Flow-graph node space: commands occupy nodes `0..n`; transaction input `k` occupies node
    // `n + k`; the gas coin occupies node `gas_node = n + m` (`m = txn.inputs.len()`); total node
    // count is `n + m + 1`. Any input/gas node never referenced by a command stays an isolated
    // singleton and is harmless.
    let Some(gas_node) = n.checked_add(txn.inputs.len()) else {
        invariant_violation!("gas-coin flow node index overflow");
    };
    let Some(node_count) = gas_node.checked_add(1) else {
        invariant_violation!("flow-graph node count overflow");
    };

    let mut uf: UnionFind<usize> = UnionFind::new(node_count);
    assert_invariant!(
        uf.len() == node_count,
        "union-find length does not match expected flow-graph node count"
    );

    build_components::<E>(&txn.commands, &txn.inputs, n, gas_node, &mut uf)?;
    let per_root_linkage =
        unify_per_component_linkages::<E>(&txn.commands, &mut uf, linkage_analysis, package_store)?;
    write_back_linkages::<E>(&mut txn.commands, &mut uf, &per_root_linkage)?;
    Ok(())
}

/// Build the weakly-connected components of the any-value flow graph directly into `uf` by
/// unioning the endpoints of every data-flow edge. Because every command is a component seed
/// (any-value flow), the components are exactly the weakly-connected components of the edge set.
///
/// Edges, all incident on command node `i` and walked from `i`'s arguments:
///   - `Argument::Result(a)` / `NestedResult(a, _)`: command `a`'s result feeds `i` (`a < i`).
///   - `Argument::Input(k)`: contributing input `k` feeds `i`, via input node `n + k`.
///   - `Argument::GasCoin`: the gas coin feeds `i`, via the gas node `gas_node = <num_commands> + <num_inputs>`.
///
/// Inputs and the gas coin are their own nodes so that every consumer of a given input or of the
/// gas coin lands in one component -- the values carry a single identity as they flow into/across
/// commands.
fn build_components<E: ExecutionErrorTrait>(
    commands: &[Command],
    inputs: &Inputs,
    n: usize,
    gas_node: usize,
    uf: &mut UnionFind<usize>,
) -> Result<(), E> {
    for (i, cmd) in commands.iter().enumerate() {
        for (arg_idx, arg) in cmd.arguments().enumerate() {
            let other = match arg {
                Argument::Result(a) | Argument::NestedResult(a, _) => {
                    let result_idx = *a;
                    let a = checked_as!(result_idx, usize)?;
                    if a >= i {
                        return Err(command_argument_error(
                            CommandArgumentError::IndexOutOfBounds { idx: result_idx },
                            arg_idx,
                        )
                        .with_command_index(i)
                        .into());
                    }
                    a
                }
                Argument::GasCoin => gas_node,
                Argument::Input(input_idx) => {
                    let k = checked_as!(*input_idx, usize)?;
                    let Some((input_arg, _)) = inputs.get(k) else {
                        return Err(command_argument_error(
                            CommandArgumentError::IndexOutOfBounds { idx: *input_idx },
                            arg_idx,
                        )
                        .with_command_index(i)
                        .into());
                    };
                    if !is_contributing_input(input_arg) {
                        continue;
                    }
                    // `k < inputs.len()` (guaranteed above), so `n + k < gas_node < node_count`.
                    let Some(node) = n.checked_add(k) else {
                        invariant_violation!("input flow node index overflow");
                    };
                    node
                }
            };
            // `try_union` bounds-checks both indices and returns `Err(bad_index)` if `i` or
            // `other` is out of bounds. Both indices are < `node_count` by construction, so this
            // is defensive.
            if uf.try_union(i, other).is_err() {
                invariant_violation!("union-find index out of bounds (command {i}, other {other})");
            }
        }
    }
    Ok(())
}

fn is_contributing_input(input: &InputArg) -> bool {
    match input {
        InputArg::Object(_)
        | InputArg::Pure(_)
        | InputArg::Receiving(_)
        | InputArg::FundsWithdrawal(_) => true,
    }
}

/// For each component containing at least one MoveCall, build a fresh resolution table seeded
/// with native packages, fold every member MoveCall's constraints into it (same emission rules
/// as `analysis::LinkageAnalyzer::compute_call_linkage_`), and finalize to a shared
/// `ExecutableLinkage`. Returned map is keyed by union-find root; components with no MoveCalls
/// are absent (no linkage needed at runtime).
fn unify_per_component_linkages<E: ExecutionErrorTrait>(
    commands: &[Command],
    uf: &mut UnionFind<usize>,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &dyn PackageStore,
) -> Result<BTreeMap<usize, ExecutableLinkage>, E> {
    use std::collections::btree_map::Entry;
    let mut tables: BTreeMap<usize, ResolutionTable> = BTreeMap::new();

    for (i, cmd) in commands.iter().enumerate() {
        let Command::MoveCall(mc) = cmd else {
            continue;
        };

        // `try_find_mut` returns `None` on an out-of-bounds index.
        // `i < commands.len() <= uf.len()` should always hold so this is defensive.
        let Some(root) = uf.try_find_mut(i) else {
            invariant_violation!("union-find index {i} out of bounds when unifying linkages");
        };
        let table = match tables.entry(root) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(
                linkage_analysis
                    .config()
                    .resolution_table_with_native_packages::<E>(package_store)?,
            ),
        };
        add_call_to_table::<E>(table, &mc.function, package_store)
            .map_err(|e| e.with_command_index(i))?;
    }
    let per_root_linkage: BTreeMap<usize, ExecutableLinkage> = tables
        .into_iter()
        .map(|(root, table)| {
            (
                root,
                ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(table)),
            )
        })
        .collect();
    // Each entry here was seeded with native packages and folded at least one MoveCall's own
    // package into it, so an empty linkage map would mean we produced a linkage for a component
    // that never had a call folded in.
    for (root, linkage) in per_root_linkage.iter() {
        assert_invariant!(
            !linkage.0.linkage.is_empty(),
            "per-component linkage for component rooted at command {root} is empty"
        );
    }
    Ok(per_root_linkage)
}

fn add_call_to_table<E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    function: &LoadedFunction,
    store: &dyn PackageStore,
) -> Result<(), E> {
    let dep_resolution_fn = match function.visibility {
        Visibility::Public => VersionConstraint::at_least,
        Visibility::Private | Visibility::Friend => VersionConstraint::exact,
    };
    let package: ObjectID = (*function.version_mid.address()).into();
    add_package::<E>(
        &package,
        store,
        resolution_table,
        VersionConstraint::exact,
        dep_resolution_fn,
    )?;
    for type_defining_id in function
        .type_arguments
        .iter()
        .flat_map(|ty| ty.all_addresses())
    {
        add_package::<E>(
            &ObjectID::from(type_defining_id),
            store,
            resolution_table,
            VersionConstraint::at_least,
            VersionConstraint::at_least,
        )?;
    }
    Ok(())
}

/// Overwrite `LoadedFunction.linkage` on each MoveCall with its component's shared linkage.
/// Non-MoveCall commands have no linkage slot and are skipped; components containing no
/// MoveCall have no linkage and require no write-back.
fn write_back_linkages<E: ExecutionErrorTrait>(
    commands: &mut [Command],
    uf: &mut UnionFind<usize>,
    per_root_linkage: &BTreeMap<usize, ExecutableLinkage>,
) -> Result<(), E> {
    // Invariants enforced here, both defensive:
    //   (1) every root in `per_root_linkage` is consumed by at least one MoveCall — i.e., we
    //       did not compute a linkage for a component that contains no MoveCall.
    //   (2) every MoveCall finds a linkage at its root — i.e., we did not miss computing a
    //       linkage for a component that contains a MoveCall.
    // A non-MoveCall command living in a MoveCall's component is fine; we only assert at the
    // root/component granularity.
    let mut consumed_roots: BTreeSet<usize> = BTreeSet::new();
    for (i, cmd) in commands.iter_mut().enumerate() {
        // `try_find_mut` returns `None` on an out-of-bounds index.
        // `i < commands.len() <= uf.len()` should always hold, so this is defensive.
        let Some(root) = uf.try_find_mut(i) else {
            invariant_violation!("union-find index {i} out of bounds when writing back linkages");
        };
        if let Command::MoveCall(mc) = cmd {
            let Some(linkage) = per_root_linkage.get(&root) else {
                invariant_violation!(
                    "MoveCall at command {i} (component root {root}) has no per-component \
                     linkage computed"
                );
            };
            let previous_linkage = mc.function.linkage.clone();

            assert_invariant!(
                previous_linkage.0.linkage.len() <= linkage.0.linkage.len(),
                "per-component linkage for component rooted at command {root} has fewer candidates \
                 than the per-call linkage of MoveCall at command {i}"
            );
            // Stronger than the length check above: every package the per-call linkage resolved
            // must still be present in the per-component linkage. Unification only ever adds
            // packages (the key set is a union across member calls), so a dropped key signals a
            // bug in how component constraints were folded together.
            assert_invariant!(
                previous_linkage
                    .0
                    .linkage
                    .keys()
                    .all(|k| linkage.0.linkage.contains_key(k)),
                "per-component linkage for component rooted at command {root} drops a package that \
                 the per-call linkage of MoveCall at command {i} had resolved"
            );

            mc.function.linkage = linkage.clone();
            consumed_roots.insert(root);
        }
    }
    for root in per_root_linkage.keys() {
        if !consumed_roots.contains(root) {
            invariant_violation!(
                "per-component linkage computed for component (root {root}) that contains \
                 no MoveCall"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Tests for the graph core: `build_components` unions every data-flow edge into a union-find,
    //! yielding the weakly-connected components of the any-value flow graph. The
    //! linkage-unification steps (`unify_per_component_linkages`, `write_back_linkages`) require a
    //! populated `PackageStore` and are exercised by the adapter's execution-level tests instead.

    use super::*;
    use crate::static_programmable_transactions::loading::ast::InputType;
    use sui_types::error::ExecutionError;
    use sui_types::execution_status::ExecutionErrorKind;

    /// A command with the given arguments. `MakeMoveVec` keeps every argument in a single flat
    /// list (`arguments()` yields them in order) with no MoveCall/linkage machinery, which is all
    /// `build_components` looks at.
    fn cmd(args: Vec<Argument>) -> Command {
        Command::MakeMoveVec(None, args)
    }

    fn pure_input() -> (InputArg, InputType) {
        (InputArg::Pure(vec![]), InputType::Bytes)
    }

    /// Partition `0..n` into the components induced by `uf`.
    fn components(uf: &mut UnionFind<usize>, n: usize) -> BTreeSet<BTreeSet<usize>> {
        let mut groups: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for i in 0..n {
            groups.entry(uf.find_mut(i)).or_default().insert(i);
        }
        groups.into_values().collect()
    }

    fn set<T: Ord>(items: impl IntoIterator<Item = T>) -> BTreeSet<T> {
        items.into_iter().collect()
    }

    fn is_index_oob(err: &ExecutionError) -> bool {
        matches!(
            err.kind(),
            ExecutionErrorKind::CommandArgumentError {
                kind: CommandArgumentError::IndexOutOfBounds { .. },
                ..
            }
        )
    }

    /// Run `build_components` over `commands`/`inputs` and return the resulting partition of the
    /// command nodes `0..n`. Sizes the node space the same way `refine_per_component_linkage` does.
    fn component_partition(
        commands: &[Command],
        inputs: &Inputs,
    ) -> Result<BTreeSet<BTreeSet<usize>>, ExecutionError> {
        let n = commands.len();
        let gas_node = n.checked_add(inputs.len()).unwrap();
        let node_count = gas_node.checked_add(1).unwrap();
        let mut uf: UnionFind<usize> = UnionFind::new(node_count);
        build_components::<ExecutionError>(commands, inputs, n, gas_node, &mut uf)?;
        Ok(components(&mut uf, n))
    }

    #[test]
    fn results_diamond_merges_into_one_component() {
        // c1 and c2 both consume c0; c3 consumes c1 and c2. All four collapse into one component.
        let commands = vec![
            cmd(vec![]),
            cmd(vec![Argument::Result(0)]),
            cmd(vec![Argument::Result(0)]),
            cmd(vec![Argument::Result(1), Argument::Result(2)]),
        ];
        let parts = component_partition(&commands, &vec![]).unwrap();
        assert_eq!(parts, set([set([0, 1, 2, 3])]));
    }

    #[test]
    fn nested_result_forms_edge() {
        let commands = vec![cmd(vec![]), cmd(vec![Argument::NestedResult(0, 0)])];
        let parts = component_partition(&commands, &vec![]).unwrap();
        assert_eq!(parts, set([set([0, 1])]));
    }

    #[test]
    fn disjoint_chains_stay_separate() {
        // c0 -> c1 and c2 -> c3 share no data flow.
        let commands = vec![
            cmd(vec![]),
            cmd(vec![Argument::Result(0)]),
            cmd(vec![]),
            cmd(vec![Argument::Result(2)]),
        ];
        let parts = component_partition(&commands, &vec![]).unwrap();
        assert_eq!(parts, set([set([0, 1]), set([2, 3])]));
    }

    #[test]
    fn singleton_commands_are_their_own_components() {
        let commands = vec![cmd(vec![]), cmd(vec![]), cmd(vec![])];
        let parts = component_partition(&commands, &vec![]).unwrap();
        assert_eq!(parts, set([set([0]), set([1]), set([2])]));
    }

    #[test]
    fn shared_input_merges_consumers() {
        // Two otherwise-disconnected commands both consume Input(0); the shared input node pulls
        // them into one component.
        let commands = vec![cmd(vec![Argument::Input(0)]), cmd(vec![Argument::Input(0)])];
        let parts = component_partition(&commands, &vec![pure_input()]).unwrap();
        assert_eq!(parts, set([set([0, 1])]));
    }

    #[test]
    fn shared_gas_coin_merges_consumers() {
        let commands = vec![cmd(vec![Argument::GasCoin]), cmd(vec![Argument::GasCoin])];
        let parts = component_partition(&commands, &vec![]).unwrap();
        assert_eq!(parts, set([set([0, 1])]));
    }

    #[test]
    fn distinct_inputs_do_not_merge() {
        // Each command consumes a different input, so they stay in separate components.
        let commands = vec![cmd(vec![Argument::Input(0)]), cmd(vec![Argument::Input(1)])];
        let parts = component_partition(&commands, &vec![pure_input(), pure_input()]).unwrap();
        assert_eq!(parts, set([set([0]), set([1])]));
    }

    #[test]
    fn unused_input_leaves_command_singleton() {
        // The input is never referenced: command 0 is its own singleton and no error is raised.
        let commands = vec![cmd(vec![])];
        let parts = component_partition(&commands, &vec![pure_input()]).unwrap();
        assert_eq!(parts, set([set([0])]));
    }

    #[test]
    fn rejects_self_and_forward_result_references() {
        // Result index equal to the command's own index (self reference).
        let self_ref = vec![cmd(vec![Argument::Result(0)])];
        assert!(is_index_oob(
            &component_partition(&self_ref, &vec![]).unwrap_err()
        ));

        // Result index pointing at a later command (forward reference).
        let forward = vec![cmd(vec![Argument::Result(1)]), cmd(vec![])];
        assert!(is_index_oob(
            &component_partition(&forward, &vec![]).unwrap_err()
        ));

        // NestedResult is treated the same as Result for edge purposes.
        let nested = vec![cmd(vec![Argument::NestedResult(0, 0)])];
        assert!(is_index_oob(
            &component_partition(&nested, &vec![]).unwrap_err()
        ));
    }

    #[test]
    fn rejects_out_of_bounds_input() {
        let commands = vec![cmd(vec![Argument::Input(5)])];
        assert!(is_index_oob(
            &component_partition(&commands, &vec![pure_input()]).unwrap_err()
        ));
    }
}
