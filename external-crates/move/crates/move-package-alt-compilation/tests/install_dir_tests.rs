// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use move_package_alt::flavor::vanilla::{Vanilla, default_environment};
use move_package_alt::package::layout::SourcePackageLayout;
use move_package_alt_compilation::{build_config::BuildConfig, compile_package};

fn create_test_package(dir: &Path) -> std::io::Result<()> {
    let toml_content = r#"
[package]
name = "test_package"
edition = "2024"
"#;
    fs::write(dir.join("Move.toml"), toml_content)?;

    let sources_dir = dir.join(SourcePackageLayout::Sources.path());
    fs::create_dir_all(&sources_dir)?;

    let module_content = r#"
module test_package::test_module {
    public fun test_function(): u64 {
        42
    }
}
"#;
    fs::write(sources_dir.join("test_module.move"), module_content)?;

    Ok(())
}

#[tokio::test]
async fn test_install_dir_creates_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_path = temp_dir.path().join("test_package");
    fs::create_dir(&package_path).expect("Failed to create package dir");

    create_test_package(&package_path).expect("Failed to create test package");

    let install_dir = PathBuf::from("custom_install");
    let build_config = BuildConfig {
        install_dir: Some(install_dir.clone()),
        default_flavor: Some(move_compiler::editions::Flavor::Core),
        ..Default::default()
    };

    let env = default_environment();

    let result =
        compile_package::<_, Vanilla>(&package_path, &build_config, &env, &mut Vec::new()).await;

    assert!(result.is_ok(), "Compilation should succeed");

    assert!(
        package_path.join(&install_dir).exists(),
        "Install dir should be created"
    );
    assert!(
        package_path.join(&install_dir).join("build").exists(),
        "Build directory should exist"
    );

    assert!(
        package_path
            .join(&install_dir)
            .join("build")
            .join("test_package")
            .exists(),
        "Package build directory should exist"
    );
}

#[tokio::test]
async fn test_install_dir_relative_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_path = temp_dir.path().join("test_package");
    fs::create_dir(&package_path).expect("Failed to create package dir");

    create_test_package(&package_path).expect("Failed to create test package");

    let relative_install_dir = PathBuf::from("../install_output");

    let build_config = BuildConfig {
        install_dir: Some(relative_install_dir.clone()),
        default_flavor: Some(move_compiler::editions::Flavor::Core),
        ..Default::default()
    };

    let env = default_environment();
    let mut output = Cursor::new(Vec::new());

    let result =
        compile_package::<_, Vanilla>(&package_path, &build_config, &env, &mut output).await;

    assert!(result.is_ok(), "Compilation should succeed");

    let expected_install_path = package_path.join("../install_output");
    assert!(
        expected_install_path.exists(),
        "Install dir should be created at relative path"
    );
    assert!(
        expected_install_path.join("build").exists(),
        "Build directory should exist at relative path"
    );
}

#[tokio::test]
async fn test_install_dir_absolute_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_path = temp_dir.path().join("test_package");
    fs::create_dir(&package_path).expect("Failed to create package dir");

    create_test_package(&package_path).expect("Failed to create test package");

    let absolute_install_dir = temp_dir.path().join("absolute_install");
    assert!(absolute_install_dir.is_absolute());

    let build_config = BuildConfig {
        install_dir: Some(absolute_install_dir.clone()),
        default_flavor: Some(move_compiler::editions::Flavor::Core),
        ..Default::default()
    };

    let env = default_environment();
    let mut output = Cursor::new(Vec::new());

    let result =
        compile_package::<_, Vanilla>(&package_path, &build_config, &env, &mut output).await;

    assert!(result.is_ok(), "Compilation should succeed");

    assert!(
        absolute_install_dir.exists(),
        "Install dir should be created at absolute path"
    );
    assert!(
        absolute_install_dir.join("build").exists(),
        "Build directory should exist at absolute path"
    );
    assert!(
        absolute_install_dir
            .join("build")
            .join("test_package")
            .exists(),
        "Package build directory should exist at absolute path"
    );
}

#[tokio::test]
async fn test_no_install_dir_uses_default() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_path = temp_dir.path().join("test_package");
    fs::create_dir(&package_path).expect("Failed to create package dir");

    create_test_package(&package_path).expect("Failed to create test package");

    let build_config = BuildConfig {
        default_flavor: Some(move_compiler::editions::Flavor::Core),
        ..Default::default()
    };

    let env = default_environment();
    let mut output = Cursor::new(Vec::new());

    let result =
        compile_package::<_, Vanilla>(&package_path, &build_config, &env, &mut output).await;

    assert!(result.is_ok(), "Compilation should succeed");

    assert!(
        package_path.join("build").exists(),
        "Build directory should exist in package directory when no install_dir specified"
    );
    assert!(
        package_path.join("build").join("test_package").exists(),
        "Package build directory should exist in default location"
    );
}

#[tokio::test]
async fn test_install_dir_existing_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let package_path = temp_dir.path().join("test_package");
    fs::create_dir(&package_path).expect("Failed to create package dir");

    create_test_package(&package_path).expect("Failed to create test package");

    let install_dir = temp_dir.path().join("existing_install");
    fs::create_dir(&install_dir).expect("Failed to create existing install dir");

    let test_file = install_dir.join("existing_file.txt");
    fs::write(&test_file, "existing content").expect("Failed to write test file");

    let build_config = BuildConfig {
        install_dir: Some(install_dir.clone()),
        default_flavor: Some(move_compiler::editions::Flavor::Core),
        ..Default::default()
    };

    let env = default_environment();
    let mut output = Cursor::new(Vec::new());

    let result =
        compile_package::<_, Vanilla>(&package_path, &build_config, &env, &mut output).await;

    assert!(
        result.is_ok(),
        "Compilation should succeed with existing directory"
    );

    assert!(test_file.exists(), "Existing files should be preserved");
    assert!(
        install_dir.join("build").exists(),
        "Build directory should be created in existing directory"
    );
    assert!(
        install_dir.join("build").join("test_package").exists(),
        "Package build directory should exist"
    );
}
