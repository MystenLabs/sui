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

use sui_framework_build::compiled_package::BuildConfig;

const FRAMEWORK_DOCS_DIR: &str = "docs";

/// Save revision info to environment variable
fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let sui_framework_path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let move_stdlib_path = sui_framework_path.join("deps").join("move-stdlib");

    Builder::new()
        .stack_size(16 * 1024 * 1024) // build_move_package require bigger stack size on windows.
        .spawn(move || build_framework_and_stdlib(sui_framework_path, out_dir))
        .unwrap()
        .join()
        .unwrap();

    println!("cargo:rerun-if-changed=build.rs");
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

fn build_framework_and_stdlib(sui_framework_path: &Path, out_dir: PathBuf) {
    let config = MoveBuildConfig {
        generate_docs: true,
        ..Default::default()
    };
    debug_assert!(!config.test_mode);
    build_framework_and_stdlib_with_move_config(
        sui_framework_path,
        out_dir.clone(),
        "sui-framework",
        "move-stdlib",
        config,
    );
    let config = MoveBuildConfig {
        generate_docs: true,
        test_mode: true,
        ..Default::default()
    };
    build_framework_and_stdlib_with_move_config(
        sui_framework_path,
        out_dir,
        "sui-framework-test",
        "move-stdlib-test",
        config,
    );
}

fn build_framework_and_stdlib_with_move_config(
    sui_framework_path: &Path,
    out_dir: PathBuf,
    framework_dir: &str,
    stdlib_dir: &str,
    config: MoveBuildConfig,
) {
    let pkg = BuildConfig {
        config,
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(sui_framework_path.to_path_buf())
    .unwrap();
    let sui_framework = pkg.get_framework_modules();
    let move_stdlib = pkg.get_stdlib_modules();

    serialize_modules_to_file(sui_framework, &out_dir.join(framework_dir)).unwrap();
    serialize_modules_to_file(move_stdlib, &out_dir.join(stdlib_dir)).unwrap();
    // write out generated docs
    // TODO: remove docs of deleted files
    for (fname, doc) in pkg.package.compiled_docs.unwrap() {
        let mut dst_path = PathBuf::from(FRAMEWORK_DOCS_DIR);
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
        "Failed to find framework or stdlib modules"
    );

    let binary = bcs::to_bytes(&serialized_modules)?;

    fs::write(file, &binary)?;

    Ok(())
}
