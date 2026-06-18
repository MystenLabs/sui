// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer,
            resolution::{ResolutionTable, VersionConstraint, add_and_unify, get_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::{Command, LoadedFunction, PackagePayload, Transaction, Type},
    },
};
use move_binary_format::file_format::Visibility;
use move_vm_runtime::validation::verification::ast::Package as VerifiedPackage;
use sui_types::{base_types::ObjectID, error::ExecutionErrorTrait};
use sui_verifier::INIT_FN_NAME;

/// Replace each command's per-call linkage with a single linkage shared by the whole transaction.
///
/// Done in two passes:
///   1. Fold every command's package and type-argument constraints into one `ResolutionTable`,
///      unifying as we go (an error here means the commands cannot agree on a single set of
///      package versions).
///   2. Write the resulting unified linkage back into every `MoveCall`.
///
/// Because all calls end up sharing one linkage, every package version selection is consistent
/// across the transaction.
pub fn refine_to_single_linkage<E: ExecutionErrorTrait>(
    txn: &mut Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &dyn PackageStore,
) -> Result<(), E> {
    let mut base_linkage = linkage_analysis
        .config()
        .resolution_table_with_native_packages::<E>(package_store)?;

    for (i, command) in txn.commands.iter().enumerate() {
        analyze_command::<E>(command, &mut base_linkage, package_store)
            .map_err(|e| e.with_command_index(i))?;
    }
    let resolved_linkage =
        ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(base_linkage));

    for (i, command) in txn.commands.iter_mut().enumerate() {
        write_back_linkage::<E>(command, &resolved_linkage).map_err(|e| e.with_command_index(i))?;
    }

    Ok(())
}

/// Fold a single command's contribution into the shared `resolution_table` (pass 1). Only commands
/// that pull packages into the runtime linkage contribute; the rest are no-ops.
fn analyze_command<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &mut ResolutionTable,
    store: &dyn PackageStore,
) -> Result<(), E> {
    match command {
        Command::MoveCall(move_call) => {
            add_call_to_table::<E>(resolution_table, &move_call.function, store)?;
        }
        Command::Publish(PackagePayload::Serialized(_), ..) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        Command::Publish(PackagePayload::Deserialized { modules, .. }, _, resolved_linkage) => {
            // A publish only affects the transaction's linkage if the package has an `init`
            // function: `init` runs as part of the publish, so its dependencies must be resolvable
            // in this transaction. Without an `init` the freshly published package is not called
            // and contributes nothing.
            //
            // NB: We presuppose here that if there is a function with the name "init" in the
            // modules being published, then it is the init function for the package. This holds
            // because we presuppose that the sui-verifier `entry_points_verifier` has already run
            // and verified that there is at most one function named "init" in the package and that
            // it has the correct signature. Additionally, even if it has not, it will eventually
            // run after this and raise an error if the "init" function does not have the correct
            // signature.
            //
            // `modules` is guaranteed to be non-empty by the `deserialize_modules` function.
            let has_init_fn = modules.iter().any(|module| {
                module.function_defs().iter().any(|func_def| {
                    let handle = module.function_handle_at(func_def.function);
                    let name = module.identifier_at(handle.name);
                    name == INIT_FN_NAME
                })
            });
            if has_init_fn {
                for resolved in resolved_linkage.linkage.values() {
                    add_and_unify(resolved, store, resolution_table, VersionConstraint::exact)?;
                }
            }
        }
        Command::MakeMoveVec(Some(ty), _) => {
            add_type_packages::<E>(resolution_table, std::iter::once(ty), store)?;
        }
        Command::MakeMoveVec(None, _) => (),
        // Currently upgrades cannot have init, and therefore do not contribute to the linkage. If
        // we want to allow init in upgrades, then we will need to analyze the init function here
        // and add its dependencies to the linkage at that time.
        Command::Upgrade(_, _, _, _, _) => (),
        Command::TransferObjects(_, _) | Command::SplitCoins(_, _) | Command::MergeCoins(_, _) => {}
    };
    Ok(())
}

/// Overwrite each `MoveCall`'s per-call linkage with the unified transaction-wide linkage (pass 2).
/// Only `MoveCall`s carry an executable linkage; the other commands need no write-back.
fn write_back_linkage<E: ExecutionErrorTrait>(
    command: &mut Command,
    ptb_linkage: &ExecutableLinkage,
) -> Result<(), E> {
    match command {
        Command::MoveCall(move_call) => {
            let previous_linkage = move_call.function.linkage.clone();
            assert_invariant!(
                previous_linkage.0.linkage.len() <= ptb_linkage.0.linkage.len(),
                "single linkage has fewer candidates than the per-call linkage of MoveCall"
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
                    .all(|k| ptb_linkage.0.linkage.contains_key(k)),
                "single linkage drops a package that the per-call linkage of MoveCall had resolved"
            );
            move_call.function.linkage = ptb_linkage.clone();
        }
        Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _)
        | Command::MakeMoveVec(_, _)
        | Command::Publish(_, _, _)
        | Command::Upgrade(_, _, _, _, _) => (),
    };
    Ok(())
}

/// Add a `MoveCall`'s target package and type-argument packages to the resolution table.
///
/// The called package itself is pinned `exact` (we must run exactly the version being called). Its
/// dependencies are constrained by the callee's visibility: a public entrypoint is a stable ABI,
/// so its dependencies may be upgraded (`at_least`); a private/`friend` entrypoint is not, so they
/// are pinned `exact`. Type-argument packages are always `at_least`, since types resolve upwards
/// to later versions. This mirrors `LinkageAnalyzer::compute_call_linkage_`.
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
    add_type_packages::<E>(resolution_table, function.type_arguments.iter(), store)
}

/// Resolve every package mentioned by `types`. Types resolve upwards to later versions, so the
/// package and its deps are both `at_least`.
fn add_type_packages<'a, E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    types: impl IntoIterator<Item = &'a Type>,
    store: &dyn PackageStore,
) -> Result<(), E> {
    for type_defining_id in types.into_iter().flat_map(|ty| ty.all_addresses()) {
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

/// Add a package and its transitive dependencies to the resolution table. The package itself
/// gets `self_resolution_fn`'s constraint; every transitive dep (per the package's linkage
/// table) gets `dep_resolution_fn`'s constraint.
fn add_package<E: ExecutionErrorTrait>(
    object_id: &ObjectID,
    store: &dyn PackageStore,
    resolution_table: &mut ResolutionTable,
    self_resolution_fn: fn(&VerifiedPackage) -> Option<VersionConstraint>,
    dep_resolution_fn: fn(&VerifiedPackage) -> Option<VersionConstraint>,
) -> Result<(), E> {
    let pkg = get_package(object_id, store)?;
    let transitive_deps = resolution_table
        .config
        .linkage_table(&pkg)
        .into_values()
        .map(ObjectID::from);
    add_and_unify(object_id, store, resolution_table, self_resolution_fn)?;
    for dep_id in transitive_deps {
        add_and_unify(&dep_id, store, resolution_table, dep_resolution_fn)?;
    }
    Ok(())
}
