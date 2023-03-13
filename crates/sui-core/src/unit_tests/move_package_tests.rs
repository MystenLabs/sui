// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::CompiledModule;

use sui_framework_build::compiled_package::BuildConfig;
use sui_types::{
    base_types::ObjectID,
    move_package::{ModuleStruct, MovePackage, UpgradeInfo},
    object::OBJECT_START_VERSION,
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID,
};

use std::{collections::BTreeMap, path::PathBuf};

macro_rules! type_origin_table {
    {$($module:ident :: $type:ident => $pkg:expr),* $(,)?} => {{
        let mut table = BTreeMap::new();
        $(
            table.insert(
                ModuleStruct {
                    module_name: stringify!($module).to_string(),
                    struct_name: stringify!($type).to_string(),
                },
                $pkg,
            );
        )*
        table
    }}
}

macro_rules! linkage_table {
    {$($original_id:expr => ($upgraded_id:expr, $version:expr)),* $(,)?} => {{
        let mut table = BTreeMap::new();
        $(
            table.insert(
                $original_id,
                UpgradeInfo {
                    upgraded_id: $upgraded_id,
                    upgraded_version: $version,
                }
            );
        )*
        table
    }}
}

#[tokio::test]
async fn test_simple_move_package_create() {
    let (std_move_pkg, sui_move_pkg) = sui_framework::make_std_sui_move_pkgs();
    let modules = build_test_modules("object_basics");
    let move_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        modules,
        u64::MAX,
        [&std_move_pkg, &sui_move_pkg],
    )
    .unwrap();

    let pkg_id = move_pkg.id();
    assert_eq!(
        move_pkg.type_origin_table(),
        &type_origin_table! {
            object_basics::Name => pkg_id,
            object_basics::NewValueEvent => pkg_id,
            object_basics::Object => pkg_id,
            object_basics::Wrapper => pkg_id,
        }
    );

    assert_eq!(
        move_pkg.linkage_table(),
        &linkage_table! {
            MOVE_STDLIB_OBJECT_ID => (MOVE_STDLIB_OBJECT_ID, OBJECT_START_VERSION),
            SUI_FRAMEWORK_OBJECT_ID => (SUI_FRAMEWORK_OBJECT_ID, OBJECT_START_VERSION),
        }
    );
}

#[tokio::test]
async fn test_simple_move_upgraded_package_create() {
    let (std_move_pkg, sui_move_pkg) = sui_framework::make_std_sui_move_pkgs();
    let modules = build_test_modules("object_basics");
    let move_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        modules,
        u64::MAX,
        [&std_move_pkg, &sui_move_pkg],
    )
    .unwrap();

    let upgraded_modules = build_test_modules("object_basics_upgraded");
    let upgraded_id = ObjectID::random();
    let upgraded_move_pkg = move_pkg
        .new_upgraded(
            upgraded_id,
            upgraded_modules,
            u64::MAX,
            [&std_move_pkg, &sui_move_pkg],
        )
        .unwrap();

    let pkg_id = move_pkg.id();
    assert_eq!(
        move_pkg.type_origin_table(),
        &type_origin_table! {
            object_basics::Name => pkg_id,
            object_basics::NewValueEvent => pkg_id,
            object_basics::Object => pkg_id,
            object_basics::Wrapper => pkg_id,
        }
    );

    assert_eq!(
        move_pkg.linkage_table(),
        &linkage_table! {
            MOVE_STDLIB_OBJECT_ID => (MOVE_STDLIB_OBJECT_ID, OBJECT_START_VERSION),
            SUI_FRAMEWORK_OBJECT_ID => (SUI_FRAMEWORK_OBJECT_ID, OBJECT_START_VERSION),
        }
    );

    assert_eq!(
        upgraded_move_pkg.type_origin_table(),
        &type_origin_table! {
            object_basics::Name => pkg_id,
            object_basics::NewValueEvent => pkg_id,
            object_basics::Object => pkg_id,
            object_basics::Wrapper => pkg_id,
            object_basics::ObjectNewVersion => upgraded_id,
        }
    );

    // the linkage table in this upgraded package is actually the same as in the original version as
    // dependencies haven't changed
    assert_eq!(
        upgraded_move_pkg.linkage_table(),
        &linkage_table! {
            MOVE_STDLIB_OBJECT_ID => (MOVE_STDLIB_OBJECT_ID, OBJECT_START_VERSION),
            SUI_FRAMEWORK_OBJECT_ID => (SUI_FRAMEWORK_OBJECT_ID, OBJECT_START_VERSION),
        }
    );
}

pub fn build_test_modules(test_dir: &str) -> Vec<CompiledModule> {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src");
    path.push("unit_tests");
    path.push("data");
    path.push(test_dir);
    sui_framework::build_move_package(&path, build_config)
        .unwrap()
        .get_modules()
        .cloned()
        .collect()
}
