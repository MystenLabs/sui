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

    let mod_name = "object_basics";
    let pkg_id = move_pkg.id();
    let type_origin_table = move_pkg.type_origin_table();
    // this package defines 4 structs: Name, NewValueEvent, Object, and Wrapper
    assert!(type_origin_table.len() == 4);
    verify_object_basics_initial_types(type_origin_table, &pkg_id, mod_name).await;

    let linkage_table = move_pkg.linkage_table();
    verify_object_basics_linkage(linkage_table).await;
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
    let storage_id = ObjectID::random();
    let upgraded_move_pkg = move_pkg
        .new_upgraded(
            storage_id,
            upgraded_modules,
            u64::MAX,
            [&std_move_pkg, &sui_move_pkg],
        )
        .unwrap();

    let mod_name = "object_basics";
    let pkg_id = move_pkg.id();
    let type_origin_table = move_pkg.type_origin_table();
    // the original version of this package defines 4 structs: Name, NewValueEvent, Object, and Wrapper
    assert!(type_origin_table.len() == 4);
    verify_object_basics_initial_types(type_origin_table, &pkg_id, mod_name).await;
    let linkage_table = move_pkg.linkage_table();
    verify_object_basics_linkage(linkage_table).await;

    let upgraded_type_origin_table = upgraded_move_pkg.type_origin_table();
    // the upgraded version of this package defines 5 structs: Name, NewValueEvent, Object, and
    // Wrapper (from the original version) and ObjectNewVersion (from the upgraded version)

    // check entries from the original package
    verify_object_basics_initial_types(upgraded_type_origin_table, &pkg_id, mod_name).await;
    // check the entry added by the upgraded package
    assert!(
        upgraded_type_origin_table
            .get(&ModuleStruct {
                module_name: mod_name.to_string(),
                struct_name: "ObjectNewVersion".to_string(),
            })
            .unwrap()
            == &storage_id
    );

    let upgraded_linkage_table = move_pkg.linkage_table();
    // the linkage table in this upgraded package is actually the same as in the original version as
    // dependencies haven't changed
    verify_object_basics_linkage(upgraded_linkage_table).await;
}

async fn verify_object_basics_initial_types(
    type_origin_table: &BTreeMap<ModuleStruct, ObjectID>,
    pkg_id: &ObjectID,
    mod_name: &str,
) {
    assert!(
        type_origin_table
            .get(&ModuleStruct {
                module_name: mod_name.to_string(),
                struct_name: "Name".to_string(),
            })
            .unwrap()
            == pkg_id
    );
    assert!(
        type_origin_table
            .get(&ModuleStruct {
                module_name: mod_name.to_string(),
                struct_name: "NewValueEvent".to_string(),
            })
            .unwrap()
            == pkg_id
    );
    assert!(
        type_origin_table
            .get(&ModuleStruct {
                module_name: mod_name.to_string(),
                struct_name: "Object".to_string(),
            })
            .unwrap()
            == pkg_id
    );
    assert!(
        type_origin_table
            .get(&ModuleStruct {
                module_name: mod_name.to_string(),
                struct_name: "Wrapper".to_string(),
            })
            .unwrap()
            == pkg_id
    );
}

async fn verify_object_basics_linkage(linkage_table: &BTreeMap<ObjectID, UpgradeInfo>) {
    // this package depends on standard library and sui framework at their original versions
    assert!(linkage_table.len() == 2);
    assert!(
        linkage_table.get(&MOVE_STDLIB_OBJECT_ID).unwrap()
            == &UpgradeInfo {
                upgraded_id: MOVE_STDLIB_OBJECT_ID,

                upgraded_version: OBJECT_START_VERSION,
            }
    );
    assert!(
        linkage_table.get(&SUI_FRAMEWORK_OBJECT_ID).unwrap()
            == &UpgradeInfo {
                upgraded_id: SUI_FRAMEWORK_OBJECT_ID,
                upgraded_version: OBJECT_START_VERSION,
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
