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
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

pub mod natives;

/// Defines a new system package at `$address` (an ObjectID), and a type with name `$Package` that
/// implements the `SystemPackage` trait to give access to the package's contents.
///
/// The package's modules are expected to be found at sub-directory `$path` of the the cargo output
/// directory.  The process of getting them there is usually managed by this crate's `build.rs`
/// script.
///
/// The remaining `$Dep` arguments reference the types for other system packages that are transitive
/// dependencies of this package.
macro_rules! define_system_package {
    ($address:expr, $Package:ident, $path:literal, [$($Dep:ident),* $(,)?]) => {
        pub struct $Package;

        impl SystemPackage for $Package {
            const ID: ObjectID = ObjectID::from_address($address);

            const BCS_BYTES: &'static [u8] = include_bytes!(concat!(env!("OUT_DIR"), "/", $path));

            fn transitive_dependencies() -> Vec<ObjectID> {
                vec![$($Dep::ID,)*]
            }

            fn as_bytes() -> Vec<Vec<u8>> {
                bcs::from_bytes($Package::BCS_BYTES).unwrap()
            }

            fn as_modules() -> Vec<CompiledModule> {
                static MODULES: Lazy<Vec<CompiledModule>> = Lazy::new(|| {
                    $Package::as_bytes().into_iter().map(|m| CompiledModule::deserialize(&m).unwrap()).collect()
                });
                Lazy::force(&MODULES).to_owned()
            }

            fn as_package() -> MovePackage {
                MovePackage::new_system(
                    OBJECT_START_VERSION,
                    $Package::as_modules(),
                    $Package::transitive_dependencies(),
                )
            }

            fn as_object() -> Object {
                Object::new_system_package(
                    $Package::as_modules(),
                    OBJECT_START_VERSION,
                    $Package::transitive_dependencies(),
                    TransactionDigest::genesis(),
                )
            }
        }
    };
}

define_system_package!(MOVE_STDLIB_ADDRESS, MoveStdlib, "move-stdlib", []);
define_system_package!(MOVE_STDLIB_ADDRESS, MoveStdlibTest, "move-stdlib-test", []);

define_system_package!(
    SUI_FRAMEWORK_ADDRESS,
    SuiFramework,
    "sui-framework",
    [MoveStdlib]
);

define_system_package!(
    SUI_FRAMEWORK_ADDRESS,
    SuiFrameworkTest,
    "sui-framework-test",
    [MoveStdlib]
);

/// Trait exposing all the various properties of a system package in a variety of different forms,
/// of increasing levels of abstraction
pub trait SystemPackage {
    const ID: ObjectID;
    const BCS_BYTES: &'static [u8];
    fn transitive_dependencies() -> Vec<ObjectID>;
    fn as_bytes() -> Vec<Vec<u8>>;
    fn as_modules() -> Vec<CompiledModule>;
    fn as_package() -> MovePackage;
    fn as_object() -> Object;
}

pub fn system_package_ids() -> Vec<ObjectID> {
    vec![MoveStdlib::ID, SuiFramework::ID]
}

pub fn make_system_modules() -> Vec<Vec<CompiledModule>> {
    vec![MoveStdlib::as_modules(), SuiFramework::as_modules()]
}

pub fn make_system_packages() -> Vec<MovePackage> {
    vec![MoveStdlib::as_package(), SuiFramework::as_package()]
}

pub fn make_system_objects() -> Vec<Object> {
    vec![MoveStdlib::as_object(), SuiFramework::as_object()]
}

pub const DEFAULT_FRAMEWORK_PATH: &str = env!("CARGO_MANIFEST_DIR");

pub fn legacy_test_cost() -> InternalGas {
    InternalGas::new(0)
}

pub fn legacy_emit_cost() -> InternalGas {
    InternalGas::new(52)
}

pub fn legacy_empty_cost() -> InternalGas {
    InternalGas::new(84)
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
