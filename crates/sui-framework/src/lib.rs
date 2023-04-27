// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::compatibility::Compatibility;
use move_binary_format::file_format::AbilitySet;
use move_binary_format::CompiledModule;
use move_core_types::gas_algebra::InternalGas;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use sui_types::base_types::ObjectRef;
use sui_types::storage::ObjectStore;
use sui_types::DEEPBOOK_OBJECT_ID;
use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    move_package::MovePackage,
    object::{Object, OBJECT_START_VERSION},
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJECT_ID,
};
use tracing::error;

/// Represents a system package in the framework, that's built from the source code inside
/// sui-framework.
#[derive(Clone, Serialize, PartialEq, Eq, Deserialize)]
pub struct SystemPackage {
    pub id: ObjectID,
    pub bytes: Vec<Vec<u8>>,
    pub dependencies: Vec<ObjectID>,
}

impl SystemPackage {
    pub fn new(id: ObjectID, raw_bytes: &'static [u8], dependencies: &[ObjectID]) -> Self {
        let bytes: Vec<Vec<u8>> = bcs::from_bytes(raw_bytes).unwrap();
        Self {
            id,
            bytes,
            dependencies: dependencies.to_vec(),
        }
    }

    pub fn id(&self) -> &ObjectID {
        &self.id
    }

    pub fn bytes(&self) -> &[Vec<u8>] {
        &self.bytes
    }

    pub fn dependencies(&self) -> &[ObjectID] {
        &self.dependencies
    }

    pub fn modules(&self) -> Vec<CompiledModule> {
        self.bytes
            .iter()
            .map(|b| CompiledModule::deserialize_with_defaults(b).unwrap())
            .collect()
    }

    pub fn genesis_move_package(&self) -> MovePackage {
        MovePackage::new_system(
            OBJECT_START_VERSION,
            &self.modules(),
            self.dependencies.iter().copied(),
        )
    }

    pub fn genesis_object(&self) -> Object {
        Object::new_system_package(
            &self.modules(),
            OBJECT_START_VERSION,
            self.dependencies.to_vec(),
            TransactionDigest::genesis(),
        )
    }
}

impl std::fmt::Debug for SystemPackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object ID: {:?}", self.id)?;
        writeln!(f, "Size: {}", self.bytes.len())?;
        writeln!(f, "Dependencies: {:?}", self.dependencies)?;
        Ok(())
    }
}

macro_rules! define_system_packages {
    ([$(($id:expr, $path:expr, $deps:expr)),* $(,)?]) => {{
        static PACKAGES: Lazy<Vec<SystemPackage>> = Lazy::new(|| {
            vec![
                $(SystemPackage::new(
                    $id,
                    include_bytes!(concat!(env!("OUT_DIR"), "/", $path)),
                    &$deps,
                )),*
            ]
        });
        &Lazy::force(&PACKAGES)
    }}
}

pub struct BuiltInFramework;
impl BuiltInFramework {
    pub fn iter_system_packages() -> impl Iterator<Item = &'static SystemPackage> {
        // All system packages in the current build should be registered here, and this is the only
        // place we need to worry about if any of them changes.
        // TODO: Is it possible to derive dependencies from the bytecode instead of manually specifying them?
        define_system_packages!([
            (MOVE_STDLIB_OBJECT_ID, "move-stdlib", []),
            (
                SUI_FRAMEWORK_OBJECT_ID,
                "sui-framework",
                [MOVE_STDLIB_OBJECT_ID]
            ),
            (
                SUI_SYSTEM_OBJECT_ID,
                "sui-system",
                [MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID]
            ),
            (
                DEEPBOOK_OBJECT_ID,
                "deepbook",
                [MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID]
            )
        ])
        .iter()
    }

    pub fn all_package_ids() -> Vec<ObjectID> {
        Self::iter_system_packages().map(|p| p.id).collect()
    }

    pub fn get_package_by_id(id: &ObjectID) -> &'static SystemPackage {
        Self::iter_system_packages().find(|s| &s.id == id).unwrap()
    }

    pub fn genesis_move_packages() -> impl Iterator<Item = MovePackage> {
        Self::iter_system_packages().map(|package| package.genesis_move_package())
    }

    pub fn genesis_objects() -> impl Iterator<Item = Object> {
        Self::iter_system_packages().map(|package| package.genesis_object())
    }
}

pub const DEFAULT_FRAMEWORK_PATH: &str = env!("CARGO_MANIFEST_DIR");

pub fn legacy_test_cost() -> InternalGas {
    InternalGas::new(0)
}

/// Check whether the framework defined by `modules` is compatible with the framework that is
/// already on-chain (i.e. stored in `object_store`) at `id`.
///
/// - Returns `None` if the current package at `id` cannot be loaded, or the compatibility check
///   fails (This is grounds not to upgrade).
/// - Panics if the object at `id` can be loaded but is not a package -- this is an invariant
///   violation.
/// - Returns the digest of the current framework (and version) if it is equivalent to the new
///   framework (indicates support for a protocol upgrade without a framework upgrade).
/// - Returns the digest of the new framework (and version) if it is compatible (indicates
///   support for a protocol upgrade with a framework upgrade).
pub async fn compare_system_package<S: ObjectStore>(
    object_store: &S,
    id: &ObjectID,
    modules: &[CompiledModule],
    dependencies: Vec<ObjectID>,
    max_binary_format_version: u32,
    no_extraneous_module_bytes: bool,
) -> Option<ObjectRef> {
    let cur_object = match object_store.get_object(id) {
        Ok(Some(cur_object)) => cur_object,

        Ok(None) => {
            // creating a new framework package--nothing to check
            return Some(
                Object::new_system_package(
                    modules,
                    // note: execution_engine assumes any system package with version OBJECT_START_VERSION is freshly created
                    // rather than upgraded
                    OBJECT_START_VERSION,
                    dependencies,
                    // Genesis is fine here, we only use it to calculate an object ref that we can use
                    // for all validators to commit to the same bytes in the update
                    TransactionDigest::genesis(),
                )
                .compute_object_reference(),
            );
        }

        Err(e) => {
            error!("Error loading framework object at {id}: {e:?}");
            return None;
        }
    };

    let cur_ref = cur_object.compute_object_reference();
    let cur_pkg = cur_object
        .data
        .try_as_package()
        .expect("Framework not package");

    let mut new_object = Object::new_system_package(
        modules,
        // Start at the same version as the current package, and increment if compatibility is
        // successful
        cur_object.version(),
        dependencies,
        cur_object.previous_transaction,
    );

    if cur_ref == new_object.compute_object_reference() {
        return Some(cur_ref);
    }

    let compatibility = Compatibility {
        check_struct_and_pub_function_linking: true,
        check_struct_layout: true,
        check_friend_linking: false,
        check_private_entry_linking: true,
        disallowed_new_abilities: AbilitySet::ALL,
        disallow_change_struct_type_params: true,
    };

    let new_pkg = new_object
        .data
        .try_as_package_mut()
        .expect("Created as package");

    let cur_normalized =
        match cur_pkg.normalize(max_binary_format_version, no_extraneous_module_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("Could not normalize existing package: {e:?}");
                return None;
            }
        };
    let mut new_normalized = new_pkg
        .normalize(max_binary_format_version, no_extraneous_module_bytes)
        .ok()?;

    for (name, cur_module) in cur_normalized {
        let Some(new_module) = new_normalized.remove(&name) else {
            return None;
        };

        if let Err(e) = compatibility.check(&cur_module, &new_module) {
            error!("Compatibility check failed, for new version of {id}: {e:?}");
            return None;
        }
    }

    new_pkg.increment_version();
    Some(new_object.compute_object_reference())
}
