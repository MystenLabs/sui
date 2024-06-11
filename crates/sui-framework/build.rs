// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::{file_format::Visibility, CompiledModule};
use move_compiler::editions::Edition;
use move_package::{BuildConfig as MoveBuildConfig, LintFlag};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use sui_move_build::{BuildConfig, SuiPackageHooks};

const DOCS_DIR: &str = "docs";
const PUBLISHED_API_DIR: &str = "published_api.txt";

/// Save revision info to environment variable
fn main() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let packages_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages");

    let bridge_path = packages_path.join("bridge");
    let deepbook_path = packages_path.join("deepbook");
    let sui_system_path = packages_path.join("sui-system");
    let sui_framework_path = packages_path.join("sui-framework");
    let move_stdlib_path = packages_path.join("move-stdlib");

    build_packages(
        &bridge_path,
        &deepbook_path,
        &sui_system_path,
        &sui_framework_path,
        &move_stdlib_path,
        &out_dir,
    );

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
    bridge_path: &Path,
    deepbook_path: &Path,
    sui_system_path: &Path,
    sui_framework_path: &Path,
    stdlib_path: &Path,
    out_dir: &Path,
) {
    let config = MoveBuildConfig {
        generate_docs: true,
        warnings_are_errors: true,
        install_dir: Some(PathBuf::from(".")),
        lint_flag: LintFlag::LEVEL_NONE,
        default_edition: Some(Edition::E2024_BETA),
        ..Default::default()
    };
    debug_assert!(!config.test_mode);
    build_packages_with_move_config(
        bridge_path,
        deepbook_path,
        sui_system_path,
        sui_framework_path,
        stdlib_path,
        out_dir,
        "bridge",
        "deepbook",
        "sui-system",
        "sui-framework",
        "move-stdlib",
        config,
        true,
    );
    let config = MoveBuildConfig {
        generate_docs: true,
        test_mode: true,
        warnings_are_errors: true,
        install_dir: Some(PathBuf::from(".")),
        lint_flag: LintFlag::LEVEL_NONE,
        default_edition: Some(Edition::E2024_BETA),
        ..Default::default()
    };
    build_packages_with_move_config(
        bridge_path,
        deepbook_path,
        sui_system_path,
        sui_framework_path,
        stdlib_path,
        out_dir,
        "bridge-test",
        "deepbook-test",
        "sui-system-test",
        "sui-framework-test",
        "move-stdlib-test",
        config,
        false,
    );
}

fn build_packages_with_move_config(
    bridge_path: &Path,
    deepbook_path: &Path,
    sui_system_path: &Path,
    sui_framework_path: &Path,
    stdlib_path: &Path,
    out_dir: &Path,
    bridge_dir: &str,
    deepbook_dir: &str,
    system_dir: &str,
    framework_dir: &str,
    stdlib_dir: &str,
    config: MoveBuildConfig,
    write_docs: bool,
) {
    let stdlib_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
    }
    .build(stdlib_path)
    .unwrap();
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

    let move_stdlib = stdlib_pkg.get_stdlib_modules();
    let sui_system = system_pkg.get_sui_system_modules();
    let sui_framework = framework_pkg.get_sui_framework_modules();
    let deepbook = deepbook_pkg.get_deepbook_modules();
    let bridge = bridge_pkg.get_bridge_modules();

    let sui_system_members =
        serialize_modules_to_file(sui_system, &out_dir.join(system_dir)).unwrap();
    let sui_framework_members =
        serialize_modules_to_file(sui_framework, &out_dir.join(framework_dir)).unwrap();
    let deepbook_members =
        serialize_modules_to_file(deepbook, &out_dir.join(deepbook_dir)).unwrap();
    let bridge_members = serialize_modules_to_file(bridge, &out_dir.join(bridge_dir)).unwrap();
    let stdlib_members = serialize_modules_to_file(move_stdlib, &out_dir.join(stdlib_dir)).unwrap();

    // write out generated docs
    if write_docs {
        // Remove the old docs directory -- in case there was a module that was deleted (could
        // happen during development).
        if Path::new(DOCS_DIR).exists() {
            std::fs::remove_dir_all(DOCS_DIR).unwrap();
        }
        let mut files_to_write = BTreeMap::new();
        relocate_docs(
            deepbook_dir,
            &deepbook_pkg.package.compiled_docs.unwrap(),
            &mut files_to_write,
        );
        relocate_docs(
            system_dir,
            &system_pkg.package.compiled_docs.unwrap(),
            &mut files_to_write,
        );
        relocate_docs(
            framework_dir,
            &framework_pkg.package.compiled_docs.unwrap(),
            &mut files_to_write,
        );
        relocate_docs(
            bridge_dir,
            &bridge_pkg.package.compiled_docs.unwrap(),
            &mut files_to_write,
        );
        for (fname, doc) in files_to_write {
            let mut dst_path = PathBuf::from(DOCS_DIR);
            dst_path.push(fname);
            fs::create_dir_all(dst_path.parent().unwrap()).unwrap();
            fs::write(dst_path, doc).unwrap();
        }

        let published_api = [
            sui_system_members.join("\n"),
            sui_framework_members.join("\n"),
            deepbook_members.join("\n"),
            bridge_members.join("\n"),
            stdlib_members.join("\n"),
        ]
        .join("\n");

        fs::write(PUBLISHED_API_DIR, published_api).unwrap();
    }
}

/// Post process the generated docs so that they are in a format that can be consumed by
/// docusaurus.
/// * Flatten out the tree-like structure of the docs directory that we generate for a package into
///   a flat list of packages;
/// * Deduplicate packages (since multiple packages could share dependencies); and
/// * Write out the package docs in a flat directory structure.
fn relocate_docs(prefix: &str, files: &[(String, String)], output: &mut BTreeMap<String, String>) {
    // Turn on multi-line mode so that `.` matches newlines, consume from the start of the file to
    // beginning of the heading, then capture the heading and replace with the yaml tag for docusaurus. E.g.,
    // ```
    // -<a name="0x2_display"></a>
    // -
    // -# Module `0x2::display`
    // -
    // +---
    // +title: Module `0x2::display`
    // +---
    //```
    let re = regex::Regex::new(r"(?s).*\n#\s+(.*?)\n").unwrap();
    for (file_name, file_content) in files {
        let path = PathBuf::from(file_name);
        let top_level = path.components().count() == 1;
        let file_name = if top_level {
            let mut new_path = PathBuf::from(prefix);
            new_path.push(file_name);
            new_path.to_string_lossy().to_string()
        } else {
            let mut new_path = PathBuf::new();
            new_path.push(path.components().skip(1).collect::<PathBuf>());
            new_path.to_string_lossy().to_string()
        };
        output.entry(file_name).or_insert_with(|| {
            re.replace_all(
                &file_content
                    .replace("../../dependencies/", "../")
                    .replace("dependencies/", "../"),
                "---\ntitle: $1\n---\n",
            )
            .to_string()
        });
    }
}

fn serialize_modules_to_file<'a>(
    modules: impl Iterator<Item = &'a CompiledModule>,
    file: &Path,
) -> Result<Vec<String>> {
    let mut serialized_modules = Vec::new();
    let mut members = vec![];
    for module in modules {
        let module_name = module.self_id().short_str_lossless();
        for def in module.struct_defs() {
            let sh = module.datatype_handle_at(def.struct_handle);
            let sn = module.identifier_at(sh.name);
            members.push(format!("{sn}\n\tpublic struct\n\t{module_name}"));
        }

        for def in module.enum_defs() {
            let eh = module.datatype_handle_at(def.enum_handle);
            let en = module.identifier_at(eh.name);
            members.push(format!("{en}\n\tpublic enum\n\t{module_name}"));
        }

        for def in module.function_defs() {
            let fh = module.function_handle_at(def.function);
            let fn_ = module.identifier_at(fh.name);
            let viz = match def.visibility {
                Visibility::Public => "public ",
                Visibility::Friend => "public(package) ",
                Visibility::Private => "",
            };
            let entry = if def.is_entry { "entry " } else { "" };
            members.push(format!("{fn_}\n\t{viz}{entry}fun\n\t{module_name}"));
        }

        let mut buf = Vec::new();
        module.serialize_with_version(module.version, &mut buf)?;
        serialized_modules.push(buf);
    }
    assert!(
        !serialized_modules.is_empty(),
        "Failed to find system or framework or stdlib modules"
    );

    let binary = bcs::to_bytes(&serialized_modules)?;

    fs::write(file, binary)?;

    Ok(members)
}
