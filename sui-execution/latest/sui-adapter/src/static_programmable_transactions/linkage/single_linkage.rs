// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{PackageMetadata, VerifiedPackageStore},
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer,
            resolution::{
                ConstraintKind, LinkageStoreResolver, MinVersionResolver, ResolutionTable,
                VersionConstraint, get_package,
            },
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::{
            Argument, Command, DeserializedPackage, InputArg, InputType, Inputs, LoadedFunction,
            PackagePayload, Transaction, Type, module_has_init,
        },
    },
};
use move_binary_format::file_format::Visibility;
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier,
    base_types::ObjectID,
    error::ExecutionErrorTrait,
    execution_status::{ExecutionErrorKind, PackageUpgradeError},
};
#[derive(Default)]
pub(crate) struct PackageVersionTargets {
    // The set of original package IDs that were resolved by a `MoveCall` in the transaction and
    // that may participate in the runtime linkage.
    pub execution_original_ids: BTreeSet<ObjectID>,
    // The set of `(original_id, package_version)` pairs that were declared by a publish or upgrade
    // command, regardless of whether that command runs an `init` and contributes to the runtime
    // linkage.
    pub publication_versions: BTreeSet<(ObjectID, u64)>,
}

/// Replace each command's per-call linkage with a single linkage shared by the whole transaction.
///
/// Done in two passes:
///   1. Fold every command's package and type-argument constraints into one `ResolutionTable`,
///      unifying as we go (an error here means the commands cannot agree on a single set of
///      package versions).
///      - Top level functions are pinned `exact`, while their dependencies are
///        pinned `exact` or `at_least` based on the visibility of the top-level function.
///        Type-argument packages are always `at_least`.
///      - Publishes and upgrades introduce their own constraints to the linkage, but only if
///        they have an `init` function (otherwise they do not contribute to the linkage). See
///        comments on each of the command arms for details on this.
///   2. Write the resulting unified linkage back into every `MoveCall`.
///
/// Because all calls end up sharing one linkage, every package version selection is consistent
/// across the transaction.
pub(crate) fn refine_to_single_linkage<E: ExecutionErrorTrait>(
    txn: &mut Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
    minversion_resolver: Option<&MinVersionResolver<'_, E>>,
) -> Result<PackageVersionTargets, E> {
    let mut base_linkage = linkage_analysis
        .config()
        .resolution_table_with_native_packages::<E, _>(package_store)?;
    let mut resolver = LinkageStoreResolver::new(package_store, minversion_resolver);
    let mut version_targets = PackageVersionTargets::default();

    for (i, command) in txn.commands.iter().enumerate() {
        if protocol_config.package_version_rules_enabled() {
            collect_package_version_targets::<E>(
                command,
                &base_linkage,
                &mut version_targets,
                &mut resolver,
            )
            .map_err(|e| e.with_command_index(i))?;
        }
        analyze_command::<E>(command, &mut base_linkage, protocol_config, &mut resolver)
            .map_err(|e| e.with_command_index(i))?;
        add_used_input_linkage::<E>(
            command.arguments(),
            &txn.inputs,
            &mut base_linkage,
            &mut resolver,
            protocol_config,
        )
        .map_err(|e| e.with_command_index(i))?;
    }

    add_withdrawal_compatibility_input_linkage::<E>(
        &txn.inputs,
        &mut base_linkage,
        &mut resolver,
        protocol_config,
    )?;

    if protocol_config.enable_order_independent_upgrade_init_linkage() {
        for (i, command) in txn.commands.iter().enumerate() {
            let Command::Upgrade(payload, _, current_package_id, _, resolved_linkage) = command
            else {
                continue;
            };
            analyze_upgrade_command::<E>(
                payload,
                current_package_id,
                resolved_linkage,
                &mut base_linkage,
                &mut resolver,
                protocol_config,
            )
            .map_err(|e| e.with_command_index(i))?;
        }
    }

    // Validate init linkage constraints before `from_resolution_table` erases the underlying
    // constraints. Minversion mismatches are transaction errors; all others are invariants.
    if protocol_config.harden_linkage_consistency() {
        for (i, command) in txn.commands.iter().enumerate() {
            validate_init_linkage_pinning::<E>(
                command,
                &base_linkage,
                package_store,
                protocol_config.enable_package_minversion(),
            )
            .map_err(|e| e.with_command_index(i))?;
        }
    }

    let resolved_linkage =
        ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(base_linkage));

    // `harden_linkage_consistency` ensures every unified-linkage entry resolves to a version.
    assert_invariant!(
        !protocol_config.harden_linkage_consistency()
            || resolved_linkage
                .0
                .linkage_resolution
                .iter()
                .all(|(_, resolution)| resolution.version.is_some()),
        "Unified linkage must resolve every package to a specific version, but found: {:?}",
        resolved_linkage
    );

    for (i, command) in txn.commands.iter_mut().enumerate() {
        write_back_linkage::<E>(command, &resolved_linkage).map_err(|e| e.with_command_index(i))?;
    }

    txn.unified_linkage = Some(resolved_linkage);
    Ok(version_targets)
}

fn add_used_input_linkage<'a, E: ExecutionErrorTrait>(
    arguments: impl IntoIterator<Item = &'a Argument>,
    inputs: &Inputs,
    resolution_table: &mut ResolutionTable,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    if !protocol_config.harden_linkage_consistency() {
        return Ok(());
    }

    for argument in arguments {
        if let Argument::Input(i) = argument
            && let Some((_, InputType::Fixed(ty))) = inputs.get(*i as usize)
        {
            add_type_packages::<E>(resolution_table, std::iter::once(ty), resolver)?;
        }
    }
    Ok(())
}

fn add_withdrawal_compatibility_input_linkage<E: ExecutionErrorTrait>(
    inputs: &Inputs,
    resolution_table: &mut ResolutionTable,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    if !protocol_config.harden_linkage_consistency() {
        return Ok(());
    }

    add_type_packages::<E>(
        resolution_table,
        inputs.iter().filter_map(|(input_arg, input_ty)| {
            if let InputArg::FundsWithdrawal(withdrawal) = input_arg
                && withdrawal.from_compatibility_object
                && let InputType::Fixed(ty) = input_ty
            {
                Some(ty)
            } else {
                None
            }
        }),
        resolver,
    )
}

/// A publish or upgrade that runs `init` executes it under its declared linkage, so every
/// dependency must be pinned exactly to the declared version in the unified linkage.
fn validate_init_linkage_pinning<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &ResolutionTable,
    store: &VerifiedPackageStore<'_>,
    minversion_enabled: bool,
) -> Result<(), E> {
    let validate_package_init_linkage = |declared_linkage: &ResolvedLinkage, err_context| {
        for (original_id, version_id) in &declared_linkage.linkage {
            match resolution_table.resolution_table.get(original_id) {
                Some(VersionConstraint::Exact(_, pinned_id)) if pinned_id == version_id => (),
                other if minversion_enabled => {
                    return Err(E::new_with_source(
                        ExecutionErrorKind::InvalidLinkage,
                        format!(
                            "{err_context} runs an `init` that requires package {original_id} at \
                            {version_id}, but the transaction linkage pins it to {other:?}"
                        ),
                    ));
                }
                other => invariant_violation!(
                    "{err_context} runs an `init` that requires package {original_id} at \
                    {version_id}, but the transaction linkage pins it to {other:?}"
                ),
            }
        }
        Ok(())
    };

    match command {
        Command::Publish(PackagePayload::Deserialized(pkg), _, resolved_linkage) => {
            if pkg.has_potential_init() {
                validate_package_init_linkage(resolved_linkage, "publish")
            } else {
                Ok(())
            }
        }
        Command::Upgrade(
            PackagePayload::Deserialized(pkg),
            _,
            current_package_id,
            _,
            resolved_linkage,
        ) => {
            if upgrade_introduces_new_init::<E>(current_package_id, &pkg.modules_with_init, store)?
            {
                validate_package_init_linkage(resolved_linkage, "upgrade")
            } else {
                Ok(())
            }
        }
        Command::Publish(PackagePayload::Serialized(_), ..) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        Command::Upgrade(PackagePayload::Serialized(_), ..) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        Command::MoveCall(_)
        | Command::MakeMoveVec(_, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => Ok(()),
    }
}

fn collect_package_version_targets<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &ResolutionTable,
    version_targets: &mut PackageVersionTargets,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
) -> Result<(), E> {
    match command {
        Command::MoveCall(move_call) => resolver.collect_original_ids(
            resolution_table,
            (*move_call.function.version_mid.address()).into(),
            &mut version_targets.execution_original_ids,
        ),
        Command::Publish(_, _, resolved_linkage)
        | Command::Upgrade(_, _, _, _, resolved_linkage) => resolver
            .collect_declared_package_versions(
                resolution_table,
                resolved_linkage.linkage.values().copied(),
                &mut version_targets.publication_versions,
            ),
        Command::MakeMoveVec(_, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => Ok(()),
    }
}

/// Fold a command's runtime-linkage contribution into the shared resolution table. Commands that
/// do not execute package code are no-ops.
fn analyze_command<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &mut ResolutionTable,
    protocol_config: &ProtocolConfig,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
) -> Result<(), E> {
    match command {
        Command::MoveCall(move_call) => {
            add_call_to_table::<E>(resolution_table, &move_call.function, resolver)?;
        }
        Command::Publish(PackagePayload::Serialized(_), ..) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        Command::Publish(PackagePayload::Deserialized(pkg), _, resolved_linkage) => {
            // A publish only contributes to runtime linkage when it runs `init`; otherwise its
            // freshly published package is not called in this transaction.
            if pkg.has_potential_init() {
                add_exact_linkage_to_table::<E>(
                    resolution_table,
                    &resolved_linkage.linkage,
                    resolver,
                )?;
            }
        }
        Command::Upgrade(_, _, _, _, _)
            if protocol_config.enable_order_independent_upgrade_init_linkage() => {}
        Command::Upgrade(payload, _, current_package_id, _, resolved_linkage) => {
            analyze_upgrade_command::<E>(
                payload,
                current_package_id,
                resolved_linkage,
                resolution_table,
                resolver,
                protocol_config,
            )?;
        }
        Command::MakeMoveVec(Some(ty), _) => {
            add_type_packages::<E>(resolution_table, std::iter::once(ty), resolver)?;
        }
        Command::MakeMoveVec(None, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => {}
    };
    Ok(())
}

/// Analyze an upgrade command's runtime-linkage contribution.
fn analyze_upgrade_command<E: ExecutionErrorTrait>(
    payload: &PackagePayload,
    current_package_id: &ObjectID,
    resolved_linkage: &ResolvedLinkage,
    resolution_table: &mut ResolutionTable,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    if !protocol_config.enable_init_on_upgrade() {
        return Ok(());
    }

    let current_pkg = get_package(current_package_id, resolver.store())?;

    assert_invariant!(
        protocol_config.enable_unified_linkage(),
        "Unified linkage must be enabled before init on upgrade is supported"
    );

    let upgrade_modules_with_init = match payload {
        PackagePayload::Serialized(_) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        PackagePayload::Deserialized(DeserializedPackage {
            modules_with_init, ..
        }) => modules_with_init,
    };

    // Whether each module already present in the current package defines an `init`.
    let current_module_inits = current_pkg
        .modules()
        .iter()
        .map(|(module_id, module)| {
            (
                module_id.name().as_str(),
                module_has_init(module.compiled_module()),
            )
        })
        .collect::<BTreeMap<_, _>>();

    // Reject upgrades where an existing module adds an `init`.
    reject_existing_module_added_init::<E>(&current_module_inits, upgrade_modules_with_init)?;

    // Only newly introduced modules with an `init` contribute to the runtime linkage.
    if has_new_module_init(
        current_module_inits.keys().copied().collect(),
        upgrade_modules_with_init,
    ) {
        add_upgrade_init_linkage_to_table::<E>(
            resolution_table,
            current_package_id,
            &resolved_linkage.linkage,
            resolver,
            protocol_config,
        )?;
    }

    Ok(())
}

fn add_exact_linkage_to_table<E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    linkage: &BTreeMap<ObjectID, ObjectID>,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
) -> Result<(), E> {
    for resolved in linkage.values() {
        resolver.resolve_flattened_linkage_entry(
            resolution_table,
            *resolved,
            ConstraintKind::Exact,
            ConstraintKind::Exact,
        )?;
    }
    Ok(())
}

/// Reject an upgrade in which a module that already exists in the current package (and did not
/// previously define an `init`) introduces one. Only the upgraded `init`-defining module names are
/// looked up in the current package.
fn reject_existing_module_added_init<E: ExecutionErrorTrait>(
    current_module_inits: &BTreeMap<&str, bool>,
    upgrade_modules_with_init: &BTreeSet<Identifier>,
) -> Result<(), E> {
    for module_name in upgrade_modules_with_init {
        if current_module_inits.get(module_name.as_str()) == Some(&false) {
            return Err(<E>::from_kind(ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
            }));
        }
    }
    Ok(())
}

/// Return true if the upgrade introduces at least one new module (absent from the current package)
/// that defines an `init` function. Existing modules never count (rejected by
/// `reject_existing_module_added_init`).
fn has_new_module_init(
    current_module_names: BTreeSet<&str>,
    upgrade_modules_with_init: &BTreeSet<Identifier>,
) -> bool {
    upgrade_modules_with_init
        .iter()
        .any(|module_name| !current_module_names.contains(module_name.as_str()))
}

/// Whether this upgrade introduces a module that is absent from the current package and defines an
/// `init` -- i.e. whether this upgrade will run an `init`.
pub(crate) fn upgrade_introduces_new_init<E: ExecutionErrorTrait>(
    current_package_id: &ObjectID,
    upgrade_modules_with_init: &BTreeSet<Identifier>,
    store: &VerifiedPackageStore<'_>,
) -> Result<bool, E> {
    let current_pkg = get_package(current_package_id, store)?;
    Ok(has_new_module_init(
        current_pkg
            .modules()
            .keys()
            .map(|module_id| module_id.name().as_str())
            .collect(),
        upgrade_modules_with_init,
    ))
}

/// Add the linkage constraints introduced by an upgrade with a new-module `init`.
///
/// There are two cases based on whether the upgraded package already participates in the
/// transaction-wide (Lumpy) linkage:
///
/// - If the upgraded package's original ID is not already in the resolution table, the upgrade is
///   treated like a fresh publish-with-init: every entry of its linkage is added as an `exact`
///   constraint.
/// - If the upgraded package's original ID is in the resolution table, then for every
///   `(original_id, package_version)` in the upgrade linkage either:
///   a. `original_id` is not in the existing Lumpy linkage, so an
///   `original_id -> exact(package_version)` constraint is introduced; or
///   b. it is in the existing Lumpy linkage, in which case the resolved package ID must equal
///   `package_version`.
fn add_upgrade_init_linkage_to_table<E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    current_package_id: &ObjectID,
    linkage: &BTreeMap<ObjectID, ObjectID>,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    let current_pkg = resolver.load_package(current_package_id)?;
    let pkg_original_id = current_pkg.original_id();

    if !resolution_table
        .resolution_table
        .contains_key(&pkg_original_id)
    {
        return add_exact_linkage_to_table::<E>(resolution_table, linkage, resolver);
    }

    for (original_id, version_id) in linkage {
        let package = resolver.load_package(version_id)?;
        let selected_id = package.version_id();
        match resolution_table.resolution_table.get(original_id) {
            None => resolver.resolve_flattened_linkage_entry(
                resolution_table,
                *version_id,
                ConstraintKind::Exact,
                ConstraintKind::Exact,
            )?,
            Some(existing) if existing.object_id() == selected_id => {
                if protocol_config.harden_linkage_consistency() {
                    resolver.resolve_flattened_linkage_entry(
                        resolution_table,
                        *version_id,
                        ConstraintKind::Exact,
                        ConstraintKind::Exact,
                    )?;
                }
            }
            Some(existing) => {
                return Err(E::new_with_source(
                    ExecutionErrorKind::InvalidLinkage,
                    format!(
                        "upgrade init linkage conflicts with transaction linkage: package \
                         {original_id} resolves to {} in transaction linkage, but upgrade \
                         linkage requires {selected_id}",
                        existing.object_id(),
                    ),
                ));
            }
        }
    }

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
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
) -> Result<(), E> {
    let dependency_constraint = match function.visibility {
        Visibility::Public => ConstraintKind::AtLeast,
        Visibility::Private | Visibility::Friend => ConstraintKind::Exact,
    };
    resolver.resolve_package(
        resolution_table,
        (*function.version_mid.address()).into(),
        ConstraintKind::Exact,
        dependency_constraint,
    )?;
    add_type_packages(resolution_table, function.type_arguments.iter(), resolver)
}

/// Add every package mentioned by `types` to the resolution table. Types resolve upwards to later
/// versions, so the package and its dependencies are both `at_least`.
fn add_type_packages<'a, E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    types: impl IntoIterator<Item = &'a Type>,
    resolver: &mut LinkageStoreResolver<'_, VerifiedPackageStore<'_>, E>,
) -> Result<(), E> {
    for type_defining_id in types.into_iter().flat_map(|ty| ty.all_addresses()) {
        resolver.resolve_package(
            resolution_table,
            ObjectID::from(type_defining_id),
            ConstraintKind::AtLeast,
            ConstraintKind::AtLeast,
        )?;
    }
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
            let previous_linkage = &move_call.function.linkage;
            // Stronger than the length check above: every package the per-call linkage resolved
            // must still be present in the per-component linkage. Unification only ever adds
            // packages (the key set is a union across member calls), so a dropped key signals a
            // bug in how component constraints were folded together.
            //
            // Since `linkage`'s keys are a set, this check also implies that
            // `previous_linkage.0.linkage.len() <= ptb_linkage.0.linkage.len()`.
            assert_invariant!(
                previous_linkage
                    .0
                    .linkage
                    .keys()
                    .all(|k| ptb_linkage.0.linkage.contains_key(k)),
                "single linkage drops a package that the per-call linkage of MoveCall had resolved"
            );
            debug_assert!(
                previous_linkage.0.linkage.len() <= ptb_linkage.0.linkage.len(),
                "single linkage has fewer candidates than the per-call linkage of MoveCall"
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
