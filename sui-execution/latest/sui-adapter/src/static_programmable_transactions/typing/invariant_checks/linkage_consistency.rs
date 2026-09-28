// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checks that every command agrees with the transaction-wide (unified) linkage.

use crate::{
    execution_mode::ExecutionMode,
    sp,
    static_programmable_transactions::{
        env,
        linkage::{
            resolution::get_package,
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
            single_linkage::upgrade_introduces_new_init,
        },
        loading::ast::{DeserializedPackage, LoadedFunction, PackagePayload, Type},
        typing::ast as T,
    },
};
use move_binary_format::file_format::Visibility;
use std::rc::Rc;
use sui_types::{base_types::ObjectID, error::ExecutionErrorTrait};

pub fn verify<Mode: ExecutionMode>(
    env: &env::Env<Mode>,
    tt: &T::Transaction,
    unified_linkage: Option<&ExecutableLinkage>,
) -> Result<(), Mode::Error> {
    if !env.protocol_config.harden_linkage_consistency() {
        return Ok(());
    }
    let Some(unified_linkage) = unified_linkage else {
        invariant_violation!("unified linkage is enabled but was not recorded on the transaction")
    };

    // Every package selected by the transaction linkage must be resolved to a version id,
    // and that version id must resolve back to the same original id in the resolution table.
    for (original_id, version_id) in &unified_linkage.0.linkage {
        let Some(resolution) = unified_linkage.0.linkage_resolution.get(version_id) else {
            invariant_violation!(
                "transaction linkage selects {version_id} for package {original_id}, which has no \
                 resolution entry"
            )
        };
        let resolved_original_id = resolution.original_id;
        assert_invariant!(
            resolved_original_id == *original_id,
            "transaction linkage selects {version_id} for package {original_id}, but that version \
             resolves to {resolved_original_id}"
        );
    }

    for (i, sp!(_, c)) in tt.commands.iter().enumerate() {
        verify_command::<Mode>(env, &c.command, unified_linkage)
            .map_err(|e| e.with_command_index(i))?;
    }

    Ok(())
}

fn verify_command<Mode: ExecutionMode>(
    env: &env::Env<Mode>,
    command: &T::Command__,
    unified_linkage: &ExecutableLinkage,
) -> Result<(), Mode::Error> {
    let verify_package_init_linkage = |declared_linkage: &ResolvedLinkage, err_context| {
        for (original_id, version_id) in &declared_linkage.linkage {
            let unified_version_id = unified_linkage.0.linkage.get(original_id);
            assert_invariant!(
                unified_version_id == Some(version_id),
                "{err_context} runs an `init` that requires package {original_id} at {version_id}, \
                but the transaction linkage selects {unified_version_id:?}"
            );
        }
        Ok(())
    };

    match command {
        T::Command__::MoveCall(move_call) => {
            verify_move_call::<Mode>(env, &move_call.function, unified_linkage)
        }
        T::Command__::Publish(PackagePayload::Deserialized(pkg), _, resolved_linkage) => {
            if pkg.has_potential_init() {
                verify_package_init_linkage(resolved_linkage, "publish")
            } else {
                Ok(())
            }
        }
        T::Command__::Upgrade(
            PackagePayload::Deserialized(DeserializedPackage {
                modules_with_init, ..
            }),
            _,
            current_package_id,
            _,
            resolved_linkage,
        ) => {
            if upgrade_introduces_new_init::<Mode::Error>(
                current_package_id,
                modules_with_init,
                env.linkable_store,
            )? {
                verify_package_init_linkage(resolved_linkage, "upgrade")
            } else {
                Ok(())
            }
        }
        T::Command__::Publish(PackagePayload::Serialized(_), ..) => {
            invariant_violation!(
                "Unexpected serialized package payload in linkage consistency check"
            )
        }
        T::Command__::Upgrade(PackagePayload::Serialized(_), ..) => {
            invariant_violation!(
                "Unexpected serialized package payload in linkage consistency check"
            )
        }
        T::Command__::MakeMoveVec(ty, _) => {
            verify_type_linkage::<Mode>(std::iter::once(ty), unified_linkage)
        }
        T::Command__::TransferObjects(_, _)
        | T::Command__::SplitCoins(_, _, _)
        | T::Command__::MergeCoins(_, _, _) => Ok(()),
    }
}

/// Verify that the  `MoveCall`'s root call's version is unchanged in the unified linkage, and that
/// the versions of all its dependencies in the unified linkage satisfy what the callee's
/// visibility demanded of its dependencies -- exact for non-`public` and no older for `public`.
fn verify_move_call<Mode: ExecutionMode>(
    env: &env::Env<Mode>,
    function: &LoadedFunction,
    unified_linkage: &ExecutableLinkage,
) -> Result<(), Mode::Error> {
    // Every `MoveCall` must have been rewritten to the transaction-wide linkage itself, not
    // merely to an equal copy of it.
    assert_invariant!(
        Rc::ptr_eq(&function.linkage.0, &unified_linkage.0),
        "MoveCall was not rewritten to the transaction-wide linkage"
    );

    let original_id = ObjectID::from(*function.original_mid.address());
    let version_id = ObjectID::from(*function.version_mid.address());
    let selected = unified_linkage.0.linkage.get(&original_id);
    assert_invariant!(
        selected == Some(&version_id),
        "MoveCall to {original_id} was loaded at version {version_id}, but the transaction \
         linkage selected {selected:?}"
    );

    let deps_are_pinned = match function.visibility {
        Visibility::Private | Visibility::Friend => true,
        Visibility::Public => false,
    };
    let pkg = get_package::<Mode::Error, _>(&version_id, env.linkable_store)?;
    for (dep_original_id, dep_version_id) in env.linkage_analysis.config().linkage_table(&pkg) {
        let dep_original_id = ObjectID::from(dep_original_id);
        let dep_version_id = ObjectID::from(dep_version_id);
        let selected = unified_linkage.0.linkage.get(&dep_original_id);
        if deps_are_pinned {
            assert_invariant!(
                selected == Some(&dep_version_id),
                "MoveCall to {original_id} is not public, so its dependency {dep_original_id} \
                 must be pinned to {dep_version_id}, but the transaction linkage selects \
                 {selected:?}"
            );
        } else {
            let Some(selected) = selected else {
                invariant_violation!(
                    "MoveCall to {original_id} depends on {dep_original_id}, which is absent from \
                     the transaction linkage"
                )
            };

            // Versions are read from the linkage itself, recorded while it was being computed.
            let (Some(declared), Some(got)) = (
                unified_linkage.0.resolved_version(&dep_version_id),
                unified_linkage.0.resolved_version(selected),
            ) else {
                invariant_violation!(
                    "MoveCall to {original_id} depends on {dep_original_id}, whose version was not \
                     recorded while the transaction linkage was refined"
                )
            };

            assert_invariant!(
                declared <= got,
                "MoveCall to {original_id} declares dependency {dep_original_id} at \
                 {dep_version_id} (version {declared}), but the transaction linkage selects \
                 {selected} (version {got}), which is older"
            );
        }
    }

    verify_type_linkage::<Mode>(function.type_arguments.iter(), unified_linkage)
}

/// Packages named by a type are pinned `at_least` at their defining ID and can resolve upwards. So
/// the unified linkage must contain a version of that package >= the defining ID.
fn verify_type_linkage<'a, Mode: ExecutionMode>(
    types: impl IntoIterator<Item = &'a Type>,
    unified_linkage: &ExecutableLinkage,
) -> Result<(), Mode::Error> {
    for defining_id in types
        .into_iter()
        .flat_map(|ty| ty.all_addresses())
        .map(ObjectID::from)
    {
        let Some(resolution) = unified_linkage.0.linkage_resolution.get(&defining_id) else {
            invariant_violation!(
                "a type in this command is defined by package {defining_id}, which the transaction \
                 linkage never resolved"
            )
        };
        let original_id = resolution.original_id;
        let Some(selected) = unified_linkage.0.linkage.get(&original_id) else {
            invariant_violation!(
                "a type in this command is defined by package {original_id}, which is absent from \
                 the transaction linkage"
            )
        };
        let (Some(declared), Some(got)) = (
            unified_linkage.0.resolved_version(&defining_id),
            unified_linkage.0.resolved_version(selected),
        ) else {
            invariant_violation!(
                "a type in this command is defined by package {original_id}, whose version was not \
                 recorded while the transaction linkage was refined"
            )
        };
        assert_invariant!(
            declared <= got,
            "a type in this command is defined by {defining_id} (version {declared}) of package \
             {original_id}, but the transaction linkage selects {selected} (version {got}), which \
             is older"
        );
    }
    Ok(())
}
