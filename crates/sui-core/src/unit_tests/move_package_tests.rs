// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::CompiledModule;

use sui_framework_build::compiled_package::BuildConfig;
use sui_types::{
    base_types::ObjectID,
    error::ExecutionErrorKind,
    move_package::{ModuleStruct, MovePackage, UpgradeInfo},
    object::OBJECT_START_VERSION,
};

use std::{collections::BTreeMap, path::PathBuf};

macro_rules! type_origin_table {
    {} => { BTreeMap::new() };
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
    {} => { BTreeMap::new() };
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

#[test]
fn test_new_initial() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let b_id1 = ObjectID::from_single_byte(0xb1);
    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_pkg],
    )
    .unwrap();

    let a_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("A"),
        u64::MAX,
        [&b_pkg, &c_pkg],
    )
    .unwrap();

    assert_eq!(
        a_pkg.linkage_table(),
        &linkage_table! {
            b_id1 => (b_id1, OBJECT_START_VERSION),
            c_id1 => (c_id1, OBJECT_START_VERSION),
        }
    );

    assert_eq!(
        b_pkg.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id1, OBJECT_START_VERSION),
        }
    );

    assert_eq!(c_pkg.linkage_table(), &linkage_table! {},);

    assert_eq!(
        c_pkg.type_origin_table(),
        &type_origin_table! {
            c::C => c_id1,
        }
    );
}

#[test]
fn test_upgraded() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let mut expected_version = OBJECT_START_VERSION;
    expected_version.increment();
    assert_eq!(expected_version, c_new.version());

    assert_eq!(
        c_new.type_origin_table(),
        &type_origin_table! {
            c::C => c_id1,
            c::D => c_id2,
        },
    );
}

#[test]
fn test_depending_on_upgrade() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_new],
    )
    .unwrap();

    assert_eq!(
        b_pkg.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id2, c_new.version()),
        },
    );
}

#[test]
fn test_upgrade_upgrades_linkage() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_pkg],
    )
    .unwrap();

    let b_id2 = ObjectID::from_single_byte(0xb2);
    let b_new = b_pkg
        .new_upgraded(b_id2, build_test_modules("B"), u64::MAX, [&c_new])
        .unwrap();

    assert_eq!(
        b_pkg.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id1, OBJECT_START_VERSION),
        },
    );

    assert_eq!(
        b_new.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id2, c_new.version()),
        },
    );
}

#[test]
fn test_upgrade_downngrades_linkage() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_new],
    )
    .unwrap();

    let b_id2 = ObjectID::from_single_byte(0xb2);
    let b_new = b_pkg
        .new_upgraded(b_id2, build_test_modules("B"), u64::MAX, [&c_pkg])
        .unwrap();

    assert_eq!(
        b_pkg.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id2, c_new.version()),
        },
    );

    assert_eq!(
        b_new.linkage_table(),
        &linkage_table! {
            c_id1 => (c_id1, OBJECT_START_VERSION),
        },
    );
}

#[test]
fn test_transitively_depending_on_upgrade() {
    let c_id1 = ObjectID::from_single_byte(0xc1);
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let b_id1 = ObjectID::from_single_byte(0xb1);
    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_pkg],
    )
    .unwrap();

    let a_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("A"),
        u64::MAX,
        [&b_pkg, &c_new],
    )
    .unwrap();

    assert_eq!(
        a_pkg.linkage_table(),
        &linkage_table! {
            b_id1 => (b_id1, OBJECT_START_VERSION),
            c_id1 => (c_id2, c_new.version()),
        },
    );
}

#[test]
#[should_panic]
fn test_panic_on_empty_package() {
    let _ = MovePackage::new_initial(OBJECT_START_VERSION, vec![], u64::MAX, []);
}

#[test]
fn test_fail_on_missing_dep() {
    let err = MovePackage::new_initial(OBJECT_START_VERSION, build_test_modules("B"), u64::MAX, [])
        .unwrap_err();

    assert_eq!(
        err.kind(),
        &ExecutionErrorKind::PublishUpgradeMissingImmediateDependency
    );
}

#[test]
fn test_fail_on_missing_transitive_dep() {
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_pkg],
    )
    .unwrap();

    let err = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("A"),
        u64::MAX,
        [&b_pkg],
    )
    .unwrap_err();

    assert_eq!(
        err.kind(),
        &ExecutionErrorKind::PublishUpgradeMissingIndirectDependency
    );
}

#[test]
fn test_fail_on_transitive_dependency_downgrade() {
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv1"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let c_new = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv2"), u64::MAX, [])
        .unwrap();

    let b_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("B"),
        u64::MAX,
        [&c_new],
    )
    .unwrap();

    let err = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("A"),
        u64::MAX,
        [&b_pkg, &c_pkg],
    )
    .unwrap_err();

    assert_eq!(
        err.kind(),
        &ExecutionErrorKind::PublishUpgradeDependencyDowngrade
    );
}

#[test]
fn test_fail_on_upgrade_missing_type() {
    let c_pkg = MovePackage::new_initial(
        OBJECT_START_VERSION,
        build_test_modules("Cv2"),
        u64::MAX,
        [],
    )
    .unwrap();

    let c_id2 = ObjectID::from_single_byte(0xc2);
    let err = c_pkg
        .new_upgraded(c_id2, build_test_modules("Cv1"), u64::MAX, [])
        .unwrap_err();

    assert_eq!(err.kind(), &ExecutionErrorKind::InvariantViolation);
}

pub fn build_test_modules(test_dir: &str) -> Vec<CompiledModule> {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "move_package", test_dir]);
    sui_framework::build_move_package(&path, build_config)
        .unwrap()
        .get_modules()
        .cloned()
        .collect()
}
