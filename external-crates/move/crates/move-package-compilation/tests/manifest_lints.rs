// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fs};

use move_compiler::editions::Flavor;
use move_package::{SourcePackageLayout, Vanilla};
use move_package_compilation::{
    build_config::BuildConfig, build_plan::BuildPlan, compilation::build_for_driver,
};
use tempfile::tempdir;

#[tokio::test]
async fn deny_warning_from_manifest() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(SourcePackageLayout::Sources.path())).unwrap();
    fs::write(
        dir.path().join("Move.toml"),
        r#"[package]
name = "test"
edition = "2024"

[warnings]
all = "deny"
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("sources/m.move"),
        "module test::m { fun f() { let unused = 0; } }",
    )
    .unwrap();
    let config = BuildConfig {
        default_flavor: Some(Flavor::Core),
        ..Default::default()
    };

    let root = config
        .package_loader(dir.path(), &Vanilla::default_environment(), Vanilla::new())
        .load()
        .await
        .unwrap();
    let result = BuildPlan::create(&root, &config)
        .unwrap()
        .compile_no_exit(&mut Vec::new(), |compiler| compiler);

    assert!(result.is_err());
}

#[tokio::test]
async fn inactive_profile_is_validated_before_compiler_setup() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(SourcePackageLayout::Sources.path())).unwrap();
    fs::write(
        dir.path().join("Move.toml"),
        r#"[package]
name = "test"
edition = "2024"

[lints.test]
public_entry = "deny"
"#,
    )
    .unwrap();
    let config = BuildConfig {
        default_flavor: Some(Flavor::Core),
        ..BuildConfig::default()
    };
    let root = config
        .package_loader(dir.path(), &Vanilla::default_environment(), Vanilla::new())
        .load()
        .await
        .unwrap();
    let mut output = Vec::new();
    let result: anyhow::Result<()> =
        build_for_driver(&mut output, None, &config, &root, BTreeSet::new(), |_| {
            panic!("invalid settings reached the compiler driver")
        });
    assert_eq!(
        result.unwrap_err().to_string(),
        "invalid diagnostic filter names in Move.toml:\n  [lints.test] \"public_entry\": unknown lint filter"
    );
    assert!(!String::from_utf8(output).unwrap().contains("BUILDING"));
}
