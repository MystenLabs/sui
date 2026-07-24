// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::{PackageMetadata, PackageStore, VerifiedPackageStore},
    static_programmable_transactions::{
        linkage::{
            analysis::LinkageAnalyzer,
            facts::{LinkageCommandFacts, LinkageFacts, ModuleInitFacts},
            resolution::{ResolutionTable, VersionConstraint, add_and_unify, get_package},
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        },
        loading::ast::{Command, DeserializedPackage, PackagePayload, Transaction},
    },
};
use move_binary_format::{CompiledModule, file_format::Visibility};
use std::collections::{BTreeMap, BTreeSet};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID,
    error::ExecutionErrorTrait,
    execution_status::{ExecutionErrorKind, PackageUpgradeError},
};
use sui_verifier::INIT_FN_NAME;

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
pub fn refine_to_single_linkage<E: ExecutionErrorTrait>(
    txn: &mut Transaction,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    let facts = txn
        .commands
        .iter()
        .enumerate()
        .map(|(i, command)| {
            loaded_command_facts::<E>(command, package_store).map_err(|e| e.with_command_index(i))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let resolved_linkage = compute_unified_linkage_from_facts::<E, _>(
        facts,
        linkage_analysis,
        package_store,
        protocol_config,
        None,
    )?;

    for (i, command) in txn.commands.iter_mut().enumerate() {
        write_back_linkage::<E>(command, &resolved_linkage).map_err(|e| e.with_command_index(i))?;
    }

    Ok(())
}

fn loaded_command_facts<E: ExecutionErrorTrait>(
    command: &Command,
    package_store: &VerifiedPackageStore<'_>,
) -> Result<LinkageCommandFacts, E> {
    match command {
        Command::MoveCall(move_call) => Ok(LinkageCommandFacts::MoveCall {
            package: (*move_call.function.version_mid.address()).into(),
            visibility: move_call.function.visibility,
            type_defining_ids: move_call
                .function
                .type_arguments
                .iter()
                .flat_map(|ty| ty.all_addresses())
                .map(ObjectID::from)
                .collect(),
        }),
        Command::Publish(PackagePayload::Serialized(_), ..) => {
            invariant_violation!("Unexpected serialized package payload in linkage analysis")
        }
        Command::Publish(
            PackagePayload::Deserialized(DeserializedPackage {
                deserialized_modules,
                ..
            }),
            _,
            resolved_linkage,
        ) => Ok(LinkageCommandFacts::Publish {
            has_init: deserialized_modules.iter().any(module_has_init),
            linkage: resolved_linkage.linkage.clone(),
        }),
        Command::Upgrade(payload, _, current_package_id, _, resolved_linkage) => {
            let current_pkg = get_package::<E, _>(current_package_id, package_store)?;
            // Whether each module already present in the current package defines an `init`.
            let current_module_inits = current_pkg
                .modules()
                .iter()
                .map(|(module_id, module)| {
                    (
                        module_id.name().as_str().to_owned(),
                        module_has_init(module.compiled_module()),
                    )
                })
                .collect::<BTreeMap<_, _>>();

            let new_modules = match payload {
                PackagePayload::Serialized(_) => {
                    invariant_violation!(
                        "Unexpected serialized package payload in linkage analysis"
                    )
                }
                PackagePayload::Deserialized(DeserializedPackage {
                    deserialized_modules,
                    ..
                }) => deserialized_modules.clone(),
            };

            Ok(LinkageCommandFacts::Upgrade {
                current_package_id: *current_package_id,
                current_module_inits,
                new_modules,
                linkage: resolved_linkage.linkage.clone(),
            })
        }
        Command::MakeMoveVec(Some(ty), _) => Ok(LinkageCommandFacts::MakeMoveVec {
            type_defining_ids: ty.all_addresses().into_iter().map(ObjectID::from).collect(),
        }),
        Command::MakeMoveVec(None, _)
        | Command::TransferObjects(_, _)
        | Command::SplitCoins(_, _)
        | Command::MergeCoins(_, _) => Ok(LinkageCommandFacts::Noop),
    }
}

pub(crate) fn compute_unified_linkage_from_facts<
    E: ExecutionErrorTrait,
    S: PackageStore + ?Sized,
>(
    facts: impl IntoIterator<Item = LinkageCommandFacts>,
    linkage_analysis: &LinkageAnalyzer,
    package_store: &S,
    protocol_config: &ProtocolConfig,
    mut non_type_original_ids: Option<&mut BTreeSet<ObjectID>>,
) -> Result<ExecutableLinkage, E> {
    let mut base_linkage = linkage_analysis
        .config()
        .resolution_table_with_native_packages::<E, _>(package_store)?;

    for (i, facts) in facts.into_iter().enumerate() {
        if let Some(original_ids) = non_type_original_ids.as_deref_mut() {
            collect_non_type_original_ids::<E, S>(
                &facts,
                &base_linkage,
                package_store,
                original_ids,
            )
            .map_err(|e| e.with_command_index(i))?;
        }
        analyze_command_facts::<E, S>(facts, &mut base_linkage, package_store, protocol_config)
            .map_err(|e| e.with_command_index(i))?;
    }

    Ok(ExecutableLinkage::new(
        ResolvedLinkage::from_resolution_table(base_linkage),
    ))
}

pub(crate) fn collect_non_type_original_ids<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    facts: &LinkageCommandFacts,
    resolution_table: &ResolutionTable,
    store: &S,
    original_ids: &mut BTreeSet<ObjectID>,
) -> Result<(), E> {
    match facts {
        LinkageCommandFacts::MoveCall { package, .. } => {
            let package = get_package(package, store)?;
            original_ids.insert(package.original_id());
            original_ids.extend(
                resolution_table
                    .config
                    .linkage_table(&package)
                    .into_keys()
                    .map(ObjectID::from),
            );
        }
        LinkageCommandFacts::Publish { linkage, .. }
        | LinkageCommandFacts::Upgrade { linkage, .. } => {
            original_ids.extend(linkage.keys().copied());
        }
        LinkageCommandFacts::MakeMoveVec { .. } | LinkageCommandFacts::Noop => (),
    }
    Ok(())
}

/// Fold a command's linkage facts into the shared `resolution_table` (pass 1). Only commands
/// that pull packages into the runtime linkage contribute; the rest are no-ops.
fn analyze_command_facts<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    facts: LinkageCommandFacts,
    resolution_table: &mut ResolutionTable,
    store: &S,
    protocol_config: &ProtocolConfig,
) -> Result<(), E> {
    match facts {
        LinkageCommandFacts::MoveCall {
            package,
            visibility,
            type_defining_ids,
        } => {
            add_call_facts_to_table::<E, S>(
                resolution_table,
                &package,
                visibility,
                type_defining_ids,
                store,
            )?;
        }
        LinkageCommandFacts::Publish { has_init, linkage } => {
            // A publish only affects the transaction's linkage if the package has an `init`
            // function: `init` runs as part of the publish, so its dependencies must be resolvable
            // in this transaction. Without an `init` the freshly published package is not called
            // and contributes nothing.
            //
            // NB: We presuppose here that if a published module has a function named `init`, then
            // it is the package's init function. If it does not conform to the required `init`
            // signature, entry-point verification rejects the transaction later.
            //
            // Published modules are guaranteed non-empty by package deserialization.
            if has_init {
                add_exact_linkage_facts_to_table::<E, S>(resolution_table, &linkage, store)?;
            }
        }
        LinkageCommandFacts::Upgrade {
            current_package_id,
            current_module_inits,
            new_modules,
            linkage,
        } => {
            if !protocol_config.enable_init_on_upgrade() {
                return Ok(());
            }

            assert_invariant!(
                protocol_config.enable_unified_linkage(),
                "Unified linkage must be enabled before init on upgrade is supported"
            );

            // Reject upgrades where an existing module adds an `init`.
            reject_existing_module_added_init_facts::<E>(&current_module_inits, &new_modules)?;

            // Only newly-introduced modules with an `init` contribute to the linkage.
            if has_new_module_init_facts(&current_module_inits, &new_modules) {
                add_upgrade_init_linkage_facts_to_table::<E, S>(
                    resolution_table,
                    &current_package_id,
                    &linkage,
                    store,
                )?;
            }
        }
        LinkageCommandFacts::MakeMoveVec { type_defining_ids } => {
            add_type_package_ids::<E, S>(resolution_table, type_defining_ids, store)?;
        }
        LinkageCommandFacts::Noop => (),
    }
    Ok(())
}

fn module_has_init(module: &CompiledModule) -> bool {
    module.function_defs().iter().any(|func_def| {
        let handle = module.function_handle_at(func_def.function);
        module.identifier_at(handle.name) == INIT_FN_NAME
    })
}

fn add_exact_linkage_facts_to_table<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    resolution_table: &mut ResolutionTable,
    linkage: &LinkageFacts,
    store: &S,
) -> Result<(), E> {
    for resolved in linkage.values() {
        add_and_unify(resolved, store, resolution_table, VersionConstraint::exact)?;
    }
    Ok(())
}

/// Reject an upgrade in which a module that already exists in the current package (and did not
/// previously define an `init`) introduces one.
fn reject_existing_module_added_init_facts<E: ExecutionErrorTrait>(
    current_module_inits: &ModuleInitFacts,
    new_modules: &[CompiledModule],
) -> Result<(), E> {
    for new_module in new_modules {
        let module_name = new_module
            .identifier_at(new_module.self_handle().name)
            .as_str();
        if current_module_inits.get(module_name) == Some(&false) && module_has_init(new_module) {
            return Err(<E>::from_kind(ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
            }));
        }
    }
    Ok(())
}

/// Return true if the upgrade introduces at least one new module (absent from the current package)
/// that defines an `init` function. Existing modules never count because adding an `init` to an
/// existing module is rejected by `reject_existing_module_added_init_facts`.
fn has_new_module_init_facts(
    current_module_inits: &ModuleInitFacts,
    new_modules: &[CompiledModule],
) -> bool {
    new_modules.iter().any(|new_module| {
        let module_name = new_module
            .identifier_at(new_module.self_handle().name)
            .as_str();
        current_module_inits.get(module_name).is_none() && module_has_init(new_module)
    })
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
///   `(original_id, version_id)` in the upgrade linkage either:
///   a. `original_id` is not in the existing Lumpy linkage, so an
///      `original_id -> exact(version_id)` constraint is introduced; or
///   b. it is in the existing Lumpy linkage, in which case the resolved package ID must equal
///      `version_id`.
fn add_upgrade_init_linkage_facts_to_table<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    resolution_table: &mut ResolutionTable,
    current_package_id: &ObjectID,
    linkage: &LinkageFacts,
    store: &S,
) -> Result<(), E> {
    let current_pkg = get_package(current_package_id, store)?;
    let pkg_original_id = current_pkg.original_id();

    if !resolution_table
        .resolution_table
        .contains_key(&pkg_original_id)
    {
        return add_exact_linkage_facts_to_table::<E, S>(resolution_table, linkage, store);
    }

    for (original_id, version_id) in linkage {
        match resolution_table.resolution_table.get(original_id) {
            None => {
                add_and_unify(
                    version_id,
                    store,
                    resolution_table,
                    VersionConstraint::exact,
                )?;
            }
            Some(existing) if existing.object_id() == *version_id => (),
            Some(existing) => {
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

/// Add a `MoveCall`'s target package and type-argument packages to the resolution table.
///
/// The called package is pinned `exact`. Its dependencies are `at_least` for a public entrypoint,
/// but `exact` for a private or friend entrypoint. Type-argument packages are always `at_least`,
/// since types resolve upwards to later versions. This mirrors
/// `LinkageAnalyzer::compute_call_linkage_`.
fn add_call_facts_to_table<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    resolution_table: &mut ResolutionTable,
    package: &ObjectID,
    visibility: Visibility,
    type_defining_ids: Vec<ObjectID>,
    store: &S,
) -> Result<(), E> {
    let dep_resolution_fn = match visibility {
        Visibility::Public => VersionConstraint::at_least,
        Visibility::Private | Visibility::Friend => VersionConstraint::exact,
    };
    add_package::<E, S>(
        package,
        store,
        resolution_table,
        VersionConstraint::exact,
        dep_resolution_fn,
    )?;
    add_type_package_ids::<E, S>(resolution_table, type_defining_ids, store)
}

/// Add every type-defining package to the resolution table. Types resolve upwards to later
/// versions, so the package and its dependencies are both `at_least`.
fn add_type_package_ids<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    resolution_table: &mut ResolutionTable,
    type_defining_ids: impl IntoIterator<Item = ObjectID>,
    store: &S,
) -> Result<(), E> {
    for type_defining_id in type_defining_ids {
        add_package::<E, S>(
            &type_defining_id,
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
fn add_package<E: ExecutionErrorTrait, S: PackageStore + ?Sized>(
    object_id: &ObjectID,
    store: &S,
    resolution_table: &mut ResolutionTable,
    self_resolution_fn: fn(&S::Package) -> Option<VersionConstraint>,
    dep_resolution_fn: fn(&S::Package) -> Option<VersionConstraint>,
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
