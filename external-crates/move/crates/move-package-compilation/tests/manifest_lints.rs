// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use move_compiler::editions::Flavor;
use move_package::{SourcePackageLayout, Vanilla};
use move_package_compilation::{build_config::BuildConfig, build_plan::BuildPlan};
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
