// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_package::BuildConfig as MoveBuildConfig;
use std::thread::Builder;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use sui_move_build::{BuildConfig, SuiPackageHooks};

const DOCS_DIR: &str = "docs";

/// Save revision info to environment variable
fn main() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let packages_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages");

    let bridge_path = packages_path.join("bridge");
    let deepbook_path = packages_path.join("deepbook");
    let sui_system_path = packages_path.join("sui-system");
    let sui_framework_path = packages_path.join("sui-framework");
    let bridge_path_clone = bridge_path.clone();
    let deepbook_path_clone = deepbook_path.clone();
    let sui_system_path_clone = sui_system_path.clone();
    let sui_framework_path_clone = sui_framework_path.clone();
    let move_stdlib_path = packages_path.join("move-stdlib");

    Builder::new()
        .stack_size(16 * 1024 * 1024) // build_packages require bigger stack size on windows.
        .spawn(move || {
            build_packages(
                bridge_path_clone,
                deepbook_path_clone,
                sui_system_path_clone,
                sui_framework_path_clone,
                out_dir,
            )
        })
        .unwrap()
        .join()
        .unwrap();

    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        deepbook_path.join("Move.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        deepbook_path.join("sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        bridge_path.join("Move.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        bridge_path.join("sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        sui_system_path.join("Move.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        sui_system_path.join("sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        sui_framework_path.join("Move.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        sui_framework_path.join("sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        move_stdlib_path.join("Move.toml").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        move_stdlib_path.join("sources").display()
    );
}

fn build_packages(
    bridge_path: PathBuf,
    deepbook_path: PathBuf,
    sui_system_path: PathBuf,
    sui_framework_path: PathBuf,
    out_dir: PathBuf,
) {
    let config = MoveBuildConfig {
        generate_docs: true,
        warnings_are_errors: true,
        install_dir: Some(PathBuf::from(".")),
        no_lint: true,
        ..Default::default()
    };
    debug_assert!(!config.test_mode);
    build_packages_with_move_config(
        bridge_path.clone(),
        deepbook_path.clone(),
        sui_system_path.clone(),
        sui_framework_path.clone(),
        out_dir.clone(),
        "bridge",
        "deepbook",
        "sui-system",
        "sui-framework",
        "move-stdlib",
        config,
    );
    let config = MoveBuildConfig {
        generate_docs: true,
        test_mode: true,
        warnings_are_errors: true,
        install_dir: Some(PathBuf::from(".")),
        no_lint: true,
        ..Default::default()
    };
    build_packages_with_move_config(
        bridge_path,
        deepbook_path,
        sui_system_path,
        sui_framework_path,
        out_dir,
        "bridge-test",
        "deepbook-test",
        "sui-system-test",
        "sui-framework-test",
        "move-stdlib-test",
        config,
    );
}

fn build_packages_with_move_config(
    bridge_path: PathBuf,
    deepbook_path: PathBuf,
    sui_system_path: PathBuf,
    sui_framework_path: PathBuf,
    out_dir: PathBuf,
    bridge_dir: &str,
    deepbook_dir: &str,
    system_dir: &str,
    framework_dir: &str,
    stdlib_dir: &str,
    config: MoveBuildConfig,
) {
    let framework_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(sui_framework_path)
    .unwrap();
    let system_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(sui_system_path)
    .unwrap();
    let deepbook_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(deepbook_path)
    .unwrap();
    let bridge_pkg = BuildConfig {
        config,
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(bridge_path)
    .unwrap();

    let sui_system = system_pkg.get_sui_system_modules();
    let sui_framework = framework_pkg.get_sui_framework_modules();
    let deepbook = deepbook_pkg.get_deepbook_modules();
    let bridge = bridge_pkg.get_bridge_modules();
    let move_stdlib = framework_pkg.get_stdlib_modules();

    serialize_modules_to_file(sui_system, &out_dir.join(system_dir)).unwrap();
    serialize_modules_to_file(sui_framework, &out_dir.join(framework_dir)).unwrap();
    serialize_modules_to_file(deepbook, &out_dir.join(deepbook_dir)).unwrap();
    serialize_modules_to_file(bridge, &out_dir.join(bridge_dir)).unwrap();
    serialize_modules_to_file(move_stdlib, &out_dir.join(stdlib_dir)).unwrap();
    // write out generated docs
    // TODO: remove docs of deleted files
    for (fname, doc) in deepbook_pkg.package.compiled_docs.unwrap() {
        let mut dst_path = PathBuf::from(DOCS_DIR);
        dst_path.push(fname);
        fs::write(dst_path, doc).unwrap();
    }
    for (fname, doc) in bridge_pkg.package.compiled_docs.unwrap() {
        let mut dst_path = PathBuf::from(DOCS_DIR);
        dst_path.push(fname);
        fs::write(dst_path, doc).unwrap();
    }
    for (fname, doc) in system_pkg.package.compiled_docs.unwrap() {
        let mut dst_path = PathBuf::from(DOCS_DIR);
        dst_path.push(fname);
        fs::write(dst_path, doc).unwrap();
    }
    for (fname, doc) in framework_pkg.package.compiled_docs.unwrap() {
        let mut dst_path = PathBuf::from(DOCS_DIR);
        dst_path.push(fname);
        fs::write(dst_path, doc).unwrap();
    }
}

fn serialize_modules_to_file<'a>(
    modules: impl Iterator<Item = &'a CompiledModule>,
    file: &Path,
) -> Result<()> {
    let mut serialized_modules = Vec::new();
    for module in modules {
        let mut buf = Vec::new();
        module.serialize(&mut buf)?;
        serialized_modules.push(buf);
    }
    assert!(
        !serialized_modules.is_empty(),
        "Failed to find system or framework or stdlib modules"
    );

    let binary = bcs::to_bytes(&serialized_modules)?;

    fs::write(file, binary)?;

    Ok(())
}
