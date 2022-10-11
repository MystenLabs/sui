// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_package::BuildConfig;
use std::thread::Builder;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const FRAMEWORK_DOCS_DIR: &str = "docs";

/// Save revision info to environment variable
fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let sui_framework_path = Path::new(env!("CARGO_MANIFEST_DIR"));
    let move_stdlib_path = sui_framework_path.join("deps").join("move-stdlib");

    let stdlib_path = move_stdlib_path.clone();
    let (sui_framework, move_stdlib) = Builder::new()
        .stack_size(16 * 1024 * 1024) // build_move_package require bigger stack size on windows.
        .spawn(move || build_framework_and_stdlib(sui_framework_path, &stdlib_path))
        .unwrap()
        .join()
        .unwrap();

    serialize_modules_to_file(sui_framework, &out_dir.join("sui-framework")).unwrap();
    serialize_modules_to_file(move_stdlib, &out_dir.join("move-stdlib")).unwrap();

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

fn build_framework_and_stdlib(
    sui_framework_path: &Path,
    move_stdlib_path: &Path,
) -> (Vec<CompiledModule>, Vec<CompiledModule>) {
    let config = BuildConfig {
        generate_docs: true,
        ..Default::default()
    };
    let pkg =
        sui_framework_build::build_move_package_with_deps(sui_framework_path, config).unwrap();
    let sui_framework = sui_framework_build::filter_package_modules(&pkg).unwrap();
    let move_stdlib = sui_framework_build::build_move_stdlib_modules(move_stdlib_path).unwrap();
    // copy generated docs from build/Sui/docs to docs/
    for (fname, _) in pkg.compiled_docs.unwrap() {
        let mut src_path = PathBuf::from("build");
        src_path.push("Sui");
        src_path.push("docs");
        src_path.push(fname.clone());
        let mut dst_path = PathBuf::from(FRAMEWORK_DOCS_DIR);
        dst_path.push(fname);
        fs::copy(src_path, dst_path).unwrap();
    }
    (sui_framework, move_stdlib)
}

fn serialize_modules_to_file(modules: Vec<CompiledModule>, file: &Path) -> Result<()> {
    let mut serialized_modules = Vec::new();
    for module in modules {
        let mut buf = Vec::new();
        module.serialize(&mut buf)?;
        serialized_modules.push(buf);
    }

    let binary = bcs::to_bytes(&serialized_modules)?;

    fs::write(file, &binary)?;

    Ok(())
}
