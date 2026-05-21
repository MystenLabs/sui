// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-component linkage refinement (aka SCC-U-P).
//!
//! Runs once immediately after loading (before typing). Partitions the PTB's commands into
//! components rooted at "source" commands (defined by a configurable `SourceCriterion`).
//! Every command transitively downstream of a source (under any-value data flow) joins that
//! source's component. Within each component the per-call linkages of the member MoveCalls
//! are unified into a single shared `ExecutableLinkage`, which then replaces each member's
//! `LoadedFunction.linkage`.
//!
//! Two source criteria are supported (currently):
//! - [`SourceCriterion::MutRef`]: any command whose return type is `&mut _`.
//! - [`SourceCriterion::AnyValue`]: every command is a source. Result is the weakly connected
//!   components of the any-value flow graph.

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer,
            resolution::{ResolutionTable, VersionConstraint, add_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::{Command, LoadedFunction, Transaction, Type},
    },
};
use move_binary_format::file_format::Visibility;
use petgraph::unionfind::UnionFind;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionErrorTrait, command_argument_error},
    execution_status::CommandArgumentError,
    transaction::Argument,
};

/// Which commands seed component formation under the taint-propagation framework. The chosen
/// criterion is independent of the rest of the algorithm — graph build, BFS forward closure,
/// per-component unification, and write-back are all criterion-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCriterion {
    /// A command is a source it is a mutable ref (`Type::Reference(true, _)`)
    MutRef,
    /// Every command is a source. Equivalent to weakly-connected components of the any-value
    /// flow graph.
    AnyValue,
}

/// Refine each MoveCall's per-call linkage into a per-component linkage shared across all
/// MoveCalls in its component.
pub fn refine_per_component_linkage<E: ExecutionErrorTrait>(
    txn: &mut Transaction,
    criterion: SourceCriterion,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &dyn PackageStore,
) -> Result<(), E> {
    let n = txn.commands.len();
    if n == 0 {
        return Ok(());
    }
    let predecessors = build_predecessors::<E>(&txn.commands)?;
    let sources = identify_sources(criterion, &txn.commands);
    let mut uf: UnionFind<usize> = UnionFind::new(n);
    compute_components::<E>(&predecessors, &sources, &mut uf)?;
    let per_root_linkage =
        unify_per_component_linkages::<E>(&txn.commands, &mut uf, linkage_analysis, package_store)?;
    write_back_linkages::<E>(&mut txn.commands, &mut uf, &per_root_linkage)?;
    Ok(())
}

/// For each command index `i`, list the command indices whose results feed `i`. Derived from
/// `Argument::Result(a)` and `Argument::NestedResult(a, _)` in `i`'s arguments. `Input`/`GasCoin`
/// arguments do not contribute edges between commands.
fn build_predecessors<E: ExecutionErrorTrait>(
    commands: &[Command],
) -> Result<BTreeMap<usize, Vec<usize>>, E> {
    let mut preds: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, cmd) in commands.iter().enumerate() {
        for (arg_idx, arg) in cmd.arguments().enumerate() {
            let raw_idx = match arg {
                Argument::Result(a) | Argument::NestedResult(a, _) => *a,
                Argument::GasCoin | Argument::Input(_) => continue,
            };
            let a = checked_as!(raw_idx, usize)?;
            if a >= i {
                return Err(command_argument_error(
                    CommandArgumentError::IndexOutOfBounds { idx: raw_idx },
                    arg_idx,
                )
                .with_command_index(i)
                .into());
            }
            preds.entry(i).or_default().push(a);
        }
    }
    Ok(preds)
}

/// Source identification. Iterates commands, applying the criterion-specific predicate to each
/// command's result types.
fn identify_sources(criterion: SourceCriterion, commands: &[Command]) -> Vec<usize> {
    commands
        .iter()
        .enumerate()
        .filter_map(|(i, cmd)| {
            let result_types: &[Type] = match cmd {
                Command::MoveCall(mc) => &mc.function.signature.return_,
                _ => &[],
            };
            is_source_under(criterion, result_types).then_some(i)
        })
        .collect()
}

/// Source predicate, factored from `identify_sources` for unit-testability with synthetic
/// `Vec<Type>` inputs (no need to construct a full typed AST).
fn is_source_under(criterion: SourceCriterion, result_type: &[Type]) -> bool {
    match criterion {
        SourceCriterion::MutRef => result_type
            .iter()
            .any(|ty| matches!(ty, Type::Reference(true, _))),
        SourceCriterion::AnyValue => true,
    }
}

/// Forward closure: for each source, iterative BFS down the any-value DAG, unioning every
/// reached command with the source (with a mark-and-skip optimization `expanded` set).
fn compute_components<E: ExecutionErrorTrait>(
    predecessors: &BTreeMap<usize, Vec<usize>>,
    sources: &[usize],
    uf: &mut UnionFind<usize>,
) -> Result<(), E> {
    // Transpose the predecessor DAG once to get forward edges. Only nodes that
    // are predecessors of something get an entry. Nodes with no successors are absent and
    // handled by the lookup below.
    let mut successors: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (&i, preds_i) in predecessors.iter() {
        for &a in preds_i {
            successors.entry(a).or_default().push(i);
        }
    }

    // A node is in `expanded` once some source's BFS has walked its successors. Subsequent
    // visits union but do not re-expand.
    let mut expanded: BTreeSet<usize> = BTreeSet::new();
    let mut queue: VecDeque<usize> = VecDeque::new();

    for &s in sources {
        if expanded.contains(&s) {
            continue;
        }
        queue.clear();
        queue.push_back(s);
        while let Some(node) = queue.pop_front() {
            uf.union(s, node);
            // `insert` returns true iff `node` was newly added; if it was already present,
            // some earlier source already expanded its downstream — just union and skip.
            if !expanded.insert(node) {
                continue;
            }
            for &succ in successors.get(&node).into_iter().flatten() {
                queue.push_back(succ);
            }
        }
    }
    Ok(())
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
        let root = uf.find_mut(i);
        let table = match tables.entry(root) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(
                linkage_analysis
                    .config()
                    .resolution_table_with_native_packages::<E>(package_store)?,
            ),
        };
        add_call_to_table::<E>(table, &mc.function, package_store)?;
    }
    Ok(tables
        .into_iter()
        .map(|(root, table)| {
            (
                root,
                ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(table)),
            )
        })
        .collect())
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
    for (i, cell) in commands.iter_mut().enumerate() {
        let root = uf.find_mut(i);
        if let Command::MoveCall(mc) = cell {
            let Some(linkage) = per_root_linkage.get(&root) else {
                invariant_violation!(
                    "MoveCall at command {i} (component root {root}) has no per-component \
                     linkage computed"
                );
            };
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
