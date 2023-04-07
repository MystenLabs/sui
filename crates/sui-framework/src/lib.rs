// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_core_types::gas_algebra::InternalGas;
use once_cell::sync::Lazy;
use std::path::Path;
use sui_framework_build::compiled_package::{BuildConfig, CompiledPackage};
use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    error::SuiResult,
    move_package::MovePackage,
    object::{Object, OBJECT_START_VERSION},
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_PACKAGE_ID,
};

pub mod natives;

/// Represents a system package in the framework, that's built from the source code inside
/// sui-framework.
pub struct SystemPackage {
    id: ObjectID,
    bytes: Vec<Vec<u8>>,
    dependencies: Vec<ObjectID>,
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
            .map(|b| CompiledModule::deserialize(b).unwrap())
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
                SUI_SYSTEM_PACKAGE_ID,
                "sui-system",
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

/// Wrapper of the build command that verifies the framework version. Should eventually be removed once we can
/// do this in the obvious way (via version checks)
pub fn build_move_package(path: &Path, config: BuildConfig) -> SuiResult<CompiledPackage> {
    //let test_mode = config.config.test_mode;
    let pkg = config.build(path.to_path_buf())?;
    /*if test_mode {
        pkg.verify_framework_version(get_sui_framework_test(), get_move_stdlib_test())?;
    } else {
        pkg.verify_framework_version(get_sui_framework(), get_move_stdlib())?;
    }*/
    Ok(pkg)
}
