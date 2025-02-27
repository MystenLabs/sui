// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    binary_config::BinaryConfig, compatibility::Compatibility, CompiledModule,
};
use move_core_types::gas_algebra::InternalGas;
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use std::sync::LazyLock;
use sui_types::base_types::ObjectRef;
use sui_types::storage::ObjectStore;
use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    move_package::MovePackage,
    object::{Object, OBJECT_START_VERSION},
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID,
};
use sui_types::{BRIDGE_PACKAGE_ID, DEEPBOOK_PACKAGE_ID};
use tracing::error;

/// Encapsulates a system package in the framework
pub struct SystemPackageMetadata {
    /// The name of the package (e.g. "MoveStdLib")
    pub name: String,
    /// The path within the repo to the source (e.g. "crates/sui-framework/packages/move-stdlib")
    pub path: String,
    /// The compiled bytecode and object ID of the package
    pub compiled: SystemPackage,
}

/// Encapsulates the chain-relevant data about a framework package (such as the id or compiled
/// bytecode)
#[derive(Clone, Serialize, PartialEq, Eq, Deserialize)]
pub struct SystemPackage {
    pub id: ObjectID,
    pub bytes: Vec<Vec<u8>>,
    pub dependencies: Vec<ObjectID>,
}

impl SystemPackageMetadata {
    pub fn new(
        name: impl ToString,
        path: impl ToString,
        id: ObjectID,
        raw_bytes: &'static [u8],
        dependencies: &[ObjectID],
    ) -> Self {
        SystemPackageMetadata {
            name: name.to_string(),
            path: path.to_string(),
            compiled: SystemPackage::new(id, raw_bytes, dependencies),
        }
    }
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
            TransactionDigest::genesis_marker(),
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

macro_rules! define_system_package_metadata {
    ([$(($id:expr, $name: expr, $path:expr, $deps:expr)),* $(,)?]) => {{
        static PACKAGES: LazyLock<Vec<SystemPackageMetadata>> = LazyLock::new(|| {
            vec![
                $(SystemPackageMetadata::new(
                    $name,
                    concat!("crates/sui-framework/packages/", $path),
                    $id,
                    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/packages_compiled", "/", $path)),
                    &$deps,
                )),*
            ]
        });
        &PACKAGES
    }}
}

pub struct BuiltInFramework;
impl BuiltInFramework {
    pub fn iter_system_package_metadata() -> impl Iterator<Item = &'static SystemPackageMetadata> {
        // All system packages in the current build should be registered here, and this is the only
        // place we need to worry about if any of them changes.
        // TODO: Is it possible to derive dependencies from the bytecode instead of manually specifying them?
        define_system_package_metadata!([
            (MOVE_STDLIB_PACKAGE_ID, "MoveStdlib", "move-stdlib", []),
            (
                SUI_FRAMEWORK_PACKAGE_ID,
                "Sui",
                "sui-framework",
                [MOVE_STDLIB_PACKAGE_ID]
            ),
            (
                SUI_SYSTEM_PACKAGE_ID,
                "SuiSystem",
                "sui-system",
                [MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID]
            ),
            (
                DEEPBOOK_PACKAGE_ID,
                "DeepBook",
                "deepbook",
                [MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID]
            ),
            (
                BRIDGE_PACKAGE_ID,
                "Bridge",
                "bridge",
                [
                    MOVE_STDLIB_PACKAGE_ID,
                    SUI_FRAMEWORK_PACKAGE_ID,
                    SUI_SYSTEM_PACKAGE_ID
                ]
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

    pub fn iter_system_packages() -> impl Iterator<Item = &'static SystemPackage> {
        BuiltInFramework::iter_system_package_metadata().map(|m| &m.compiled)
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
    binary_config: &BinaryConfig,
) -> Option<ObjectRef> {
    let cur_object = match object_store.get_object(id) {
        Some(cur_object) => cur_object,

        None => {
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
                    TransactionDigest::genesis_marker(),
                )
                .compute_object_reference(),
            );
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

    let compatibility = Compatibility::framework_upgrade_check();

    let new_pkg = new_object
        .data
        .try_as_package_mut()
        .expect("Created as package");

    let cur_normalized = match cur_pkg.normalize(binary_config) {
        Ok(v) => v,
        Err(e) => {
            error!("Could not normalize existing package: {e:?}");
            return None;
        }
    };
    let mut new_normalized = new_pkg.normalize(binary_config).ok()?;

    for (name, cur_module) in cur_normalized {
        let new_module = new_normalized.remove(&name)?;

        if let Err(e) = compatibility.check(&cur_module, &new_module) {
            error!("Compatibility check failed, for new version of {id}::{name}: {e:?}");
            return None;
        }
    }

    new_pkg.increment_version();
    Some(new_object.compute_object_reference())
}
