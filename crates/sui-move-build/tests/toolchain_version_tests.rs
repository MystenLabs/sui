use std::path::PathBuf;

use expect_test::expect;
use move_compiler::editions::{Edition, Flavor};
use move_package::lock_file::schema::ToolchainVersion;
use sui_move_build::{resolve_toolchain_version, BuildConfig, CompiledPackage};

#[test]
fn no_toolchain_version() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "toolchain_version",
        "fixture",
        "a_no_toolchain_version",
    ]);
    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.lock_file = Some(path.join("Move.lock"));
    let result = build_config.build(path).unwrap();
    assert_eq!((Flavor::Sui, Edition::LEGACY), flavor_and_edition(&result));
}

#[test]
fn toolchain_version_2024() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "toolchain_version",
        "fixture",
        "b_toolchain_version_2024",
    ]);
    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.lock_file = Some(path.join("Move.lock"));
    println!("running on path {:#?}", build_config.config.lock_file);
    let result = build_config.build(path).unwrap();
    assert_eq!(
        (Flavor::Sui, Edition::E2024_ALPHA),
        flavor_and_edition(&result)
    );
}

#[test]
fn toolchain_version_lock_override() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "toolchain_version",
        "fixture",
        "c_toolchain_version_lock_override",
    ]);
    // The Move.lock file sets edition `legacy` but we expect `2024.alpha` that the Move.toml overrides.
    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.lock_file = Some(path.join("Move.lock"));
    let result = build_config.build(path).unwrap();
    assert_eq!(
        (Flavor::Sui, Edition::E2024_ALPHA),
        flavor_and_edition(&result)
    );
}

fn flavor_and_edition(pkg: &CompiledPackage) -> (Flavor, Edition) {
    let flavor = pkg
        .package
        .compiled_package_info
        .build_flags
        .default_flavor
        .unwrap();
    let edition = pkg
        .package
        .compiled_package_info
        .build_flags
        .default_edition
        .unwrap();

    (flavor, edition)
}
