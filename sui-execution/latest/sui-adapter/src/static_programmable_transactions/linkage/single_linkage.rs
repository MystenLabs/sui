// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::VerifiedPackageStore,
    static_programmable_transactions::{
        linkage::{
            resolution::{
                LinkageMeter, ResolutionTable, VersionConstraint, add_and_unify, get_package,
            },
            resolved_linkage::ResolvedLinkage,
        },
        loading::ast::{DeserializedPackage, PackagePayload, module_has_init},
        preparation::ast::Command,
    },
};
use move_binary_format::file_format::Visibility;
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, TypeTag,
    base_types::ObjectID,
    error::ExecutionErrorTrait,
    execution_status::{ExecutionErrorKind, PackageUpgradeError},
};

/// A publish or upgrade that runs an `init` executes it under the linkage declared by that command
/// so every dependency the command declares must be pinned `exact`ly to the version it declared in
/// the larger unified linkage.
pub(crate) fn validate_init_linkage_pinning<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &ResolutionTable,
    store: &VerifiedPackageStore<'_>,
) -> Result<(), E> {
    let validate_package_init_linkage = |declared_linkage: &ResolvedLinkage, err_context| {
        for (original_id, version_id) in &declared_linkage.linkage {
            match resolution_table.resolution_table.get(original_id) {
                Some(VersionConstraint::Exact(_, pinned_id)) if pinned_id == version_id => (),
                other => {
                    invariant_violation!(
                        "{err_context} runs an `init` that requires package {original_id} at \
                        {version_id}, but the transaction linkage pins it to {other:?}"
                    )
                }
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
        Command::MoveCall(_)
        | Command::MakeMoveVec(_, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => Ok(()),
    }
}

/// Fold a prepared command's ordinary constraints into the transaction-wide resolution table.
/// MoveCall targets are `Exact`; their dependencies are `AtLeast` for public functions and
/// `Exact` for private/friend functions. Explicit call and vector types contribute their defining
/// packages and dependencies as `AtLeast` constraints. A publish with an initializer pins its
/// publication linkage `Exact`; a publish without an initializer contributes nothing.
///
/// Preparation calls this in command order, after collecting input constraints. Upgrades are
/// deferred so their initializer membership checks see all ordinary contributions, including
/// those from commands after the upgrade. The other commands introduce no package constraints.
pub(crate) fn analyze_command<E: ExecutionErrorTrait>(
    command: &Command,
    resolution_table: &mut ResolutionTable,
    store: &VerifiedPackageStore<'_>,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<(), E> {
    match command {
        Command::MoveCall(call) => {
            add_and_unify(
                &call.package,
                store,
                resolution_table,
                VersionConstraint::exact,
                meter,
            )?;
            let pkg = get_package(&call.package, store)?;
            let dep_resolution = match call.visibility {
                Visibility::Public => VersionConstraint::at_least,
                Visibility::Private | Visibility::Friend => VersionConstraint::exact,
            };
            for dep in resolution_table.config.linkage_table(&pkg).into_values() {
                add_and_unify(&dep.into(), store, resolution_table, dep_resolution, meter)?;
            }
            add_type_packages::<E>(resolution_table, call.type_arguments.iter(), store, meter)?;
        }
        Command::Publish(PackagePayload::Deserialized(pkg), _, resolved_linkage) => {
            // A publish only affects the transaction's linkage if the package has an `init`
            // function: `init` runs as part of the publish, so its dependencies must be resolvable
            // in this transaction. Without an `init` the freshly published package is not called
            // and contributes nothing.
            //
            // `modules` is guaranteed to be non-empty by the `deserialize_modules` function.
            if pkg.has_potential_init() {
                for resolved in resolved_linkage.linkage.values() {
                    add_and_unify(
                        resolved,
                        store,
                        resolution_table,
                        VersionConstraint::exact,
                        meter,
                    )?;
                }
            }
        }
        Command::Upgrade(_, _, _, _, _) => {}
        Command::MakeMoveVec(Some(ty), _) => {
            add_type_packages::<E>(resolution_table, std::iter::once(ty), store, meter)?;
        }
        Command::MakeMoveVec(None, _) => (),
        Command::TransferObjects(_, _) | Command::SplitCoins(_, _) | Command::MergeCoins(_, _) => {}
    };
    Ok(())
}

/// Analyze the linkage contribution of an upgrade command.
pub(crate) fn analyze_upgrade_command<E: ExecutionErrorTrait>(
    payload: &PackagePayload,
    current_package_id: &ObjectID,
    resolved_linkage: &ResolvedLinkage,
    resolution_table: &mut ResolutionTable,
    store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<(), E> {
    let current_pkg = get_package(current_package_id, store)?;

    assert_invariant!(
        protocol_config.enable_unified_linkage(),
        "Unified linkage must be enabled before init on upgrade is supported"
    );

    let PackagePayload::Deserialized(DeserializedPackage {
        modules_with_init: upgrade_modules_with_init,
        ..
    }) = payload;

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

    // reject upgrades where an existing module adds an `init`.
    reject_existing_module_added_init::<E>(&current_module_inits, upgrade_modules_with_init)?;

    // only newly-introduced modules with an `init` contribute to the linkage.
    if has_new_module_init(
        current_module_inits.keys().copied().collect(),
        upgrade_modules_with_init,
    ) {
        add_upgrade_init_linkage_to_table::<E>(
            resolution_table,
            current_package_id,
            resolved_linkage,
            store,
            protocol_config,
            meter,
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
/// that defines an `init` function. Existing modules never count (rejected by `reject_existing_module_added_init`).
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

/// Add the linkage constraints introduced by an upgrade, there are two cases based on whether the
/// upgraded package already participates in the transaction-wide (Lumpy) linkage:
///
/// - If the upgraded package's original id is not already in the resolution table, the upgrade
///   is treated like a fresh publish-with-init: every entry of its resolved linkage is added as an
///   `exact` constraint.
/// - If the upgraded package's original id is in the resolution table, then for any `(original_id,
///   version_id)` as defined in the `Upgrade` command either:
///   a. It is not in the existing Lumpy linkage, and a `original_id -> exact(version_id)` constraint is introduced; or
///   b. It is in the existing Lumpy linkage, in which case Lumpy[original_id].id must equal `version_id`, and that entry is fixed to `exact(version_id)`.
fn add_upgrade_init_linkage_to_table<E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    current_package_id: &ObjectID,
    resolved_linkage: &ResolvedLinkage,
    store: &VerifiedPackageStore<'_>,
    _protocol_config: &ProtocolConfig,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<(), E> {
    let current_pkg = get_package(current_package_id, store)?;
    let pkg_original_id: ObjectID = current_pkg.original_id().into();

    // Case 1: The upgraded package does not participate in runtime linkage. As with a fresh
    // publish with init, add its publication linkage as Exact constraints using normal unification.
    if !resolution_table
        .resolution_table
        .contains_key(&pkg_original_id)
    {
        for resolved in resolved_linkage.linkage.values() {
            add_and_unify(
                resolved,
                store,
                resolution_table,
                VersionConstraint::exact,
                meter,
            )?;
        }
        return Ok(());
    }

    // Case 2: The upgraded package already participates. Each dependency must either be absent
    // or already select the declared version; init cannot replace an existing version selection.
    for (original_id, version_id) in &resolved_linkage.linkage {
        meter()?;
        match resolution_table.resolution_table.get(original_id) {
            None => {
                // Case 2a: The dependency is absent, so introduce its declared Exact constraint.
                add_and_unify(
                    version_id,
                    store,
                    resolution_table,
                    VersionConstraint::exact,
                    meter,
                )?;
            }
            Some(existing) if existing.object_id() == *version_id => {
                // Case 2b: The selected version agrees. Tighten an AtLeast constraint to Exact,
                // or retain an existing Exact constraint, so later upgrades cannot change it.
                add_and_unify(
                    version_id,
                    store,
                    resolution_table,
                    VersionConstraint::exact,
                    meter,
                )?;
            }
            Some(existing) => {
                // Case 2c: The selected version differs. Reject even if normal unification could
                // raise an AtLeast constraint; participation requires equality with init's linkage.
                return Err(E::new_with_source(
                    ExecutionErrorKind::InvalidLinkage,
                    format!(
                        "upgrade init linkage conflicts with transaction linkage: package \
                         {original_id} resolves to {} in transaction linkage, but upgrade \
                         linkage requires {version_id}",
                        existing.object_id(),
                    ),
                ));
            }
        }
    }

    Ok(())
}

/// Resolve every package mentioned by `types`. Types resolve upwards to later versions, so the
/// package and its deps are both `at_least`.
pub(crate) fn add_type_packages<'a, E: ExecutionErrorTrait>(
    resolution_table: &mut ResolutionTable,
    types: impl IntoIterator<Item = &'a TypeTag>,
    store: &VerifiedPackageStore<'_>,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<(), E> {
    resolution_table.add_type_linkages_to_table(
        types
            .into_iter()
            .flat_map(|ty| ty.all_addresses())
            .map(ObjectID::from),
        store,
        meter,
    )
}
