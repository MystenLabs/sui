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

const CRATE_ROOT: &str = env!("CARGO_MANIFEST_DIR");
const COMPILED_PACKAGES_DIR: &str = "packages_compiled";
const DOCS_DIR: &str = "docs";
const PUBLISHED_API_FILE: &str = "published_api.txt";

#[test]
fn build_system_packages() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let tempdir = tempfile::tempdir().unwrap();
    let out_dir = if std::env::var_os("UPDATE").is_some() {
        let crate_root = Path::new(CRATE_ROOT);
        let _ = std::fs::remove_dir_all(crate_root.join(COMPILED_PACKAGES_DIR));
        let _ = std::fs::remove_dir_all(crate_root.join(DOCS_DIR));
        let _ = std::fs::remove_file(crate_root.join(PUBLISHED_API_FILE));
        crate_root
    } else {
        tempdir.path()
    };

    std::fs::create_dir_all(out_dir.join(COMPILED_PACKAGES_DIR)).unwrap();
    std::fs::create_dir_all(out_dir.join(DOCS_DIR)).unwrap();

    let packages_path = Path::new(CRATE_ROOT).join("packages");

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
        out_dir,
    );
    check_diff(Path::new(CRATE_ROOT), out_dir)
}

// Verify that checked-in values are the same as the generated ones
fn check_diff(checked_in: &Path, built: &Path) {
    for path in [COMPILED_PACKAGES_DIR, DOCS_DIR, PUBLISHED_API_FILE] {
        let output = std::process::Command::new("diff")
            .args(["--brief", "--recursive"])
            .arg(checked_in.join(path))
            .arg(built.join(path))
            .output()
            .unwrap();
        if !output.status.success() {
            let header =
                "Generated and checked-in sui-framework packages and/or docs do not match.\n\
                 Re-run with `UPDATE=1` to update checked-in packages and docs. e.g.\n\n\
                 UPDATE=1 cargo test -p sui-framework --test build-system-packages";

            panic!(
                "{header}\n\n{}\n\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
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
) {
    let stdlib_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
        chain_id: None, // Framework pkg addr is agnostic to chain, resolves from Move.toml
    }
    .build(stdlib_path)
    .unwrap();
    let framework_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
        chain_id: None, // Framework pkg addr is agnostic to chain, resolves from Move.toml
    }
    .build(sui_framework_path)
    .unwrap();
    let system_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
        chain_id: None, // Framework pkg addr is agnostic to chain, resolves from Move.toml
    }
    .build(sui_system_path)
    .unwrap();
    let deepbook_pkg = BuildConfig {
        config: config.clone(),
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
        chain_id: None, // Framework pkg addr is agnostic to chain, resolves from Move.toml
    }
    .build(deepbook_path)
    .unwrap();
    let bridge_pkg = BuildConfig {
        config,
        run_bytecode_verifier: true,
        print_diags_to_stderr: false,
        chain_id: None, // Framework pkg addr is agnostic to chain, resolves from Move.toml
    }
    .build(bridge_path)
    .unwrap();

    let move_stdlib = stdlib_pkg.get_stdlib_modules();
    let sui_system = system_pkg.get_sui_system_modules();
    let sui_framework = framework_pkg.get_sui_framework_modules();
    let deepbook = deepbook_pkg.get_deepbook_modules();
    let bridge = bridge_pkg.get_bridge_modules();

    let compiled_packages_dir = out_dir.join(COMPILED_PACKAGES_DIR);

    let sui_system_members =
        serialize_modules_to_file(sui_system, &compiled_packages_dir.join(system_dir)).unwrap();
    let sui_framework_members =
        serialize_modules_to_file(sui_framework, &compiled_packages_dir.join(framework_dir))
            .unwrap();
    let deepbook_members =
        serialize_modules_to_file(deepbook, &compiled_packages_dir.join(deepbook_dir)).unwrap();
    let bridge_members =
        serialize_modules_to_file(bridge, &compiled_packages_dir.join(bridge_dir)).unwrap();
    let stdlib_members =
        serialize_modules_to_file(move_stdlib, &compiled_packages_dir.join(stdlib_dir)).unwrap();

    // write out generated docs
    let docs_dir = out_dir.join(DOCS_DIR);
    let mut files_to_write = BTreeMap::new();
    relocate_docs(
        &stdlib_pkg.package.compiled_docs.unwrap(),
        &mut files_to_write,
    );
    relocate_docs(
        &deepbook_pkg.package.compiled_docs.unwrap(),
        &mut files_to_write,
    );
    relocate_docs(
        &system_pkg.package.compiled_docs.unwrap(),
        &mut files_to_write,
    );
    relocate_docs(
        &framework_pkg.package.compiled_docs.unwrap(),
        &mut files_to_write,
    );
    relocate_docs(
        &bridge_pkg.package.compiled_docs.unwrap(),
        &mut files_to_write,
    );
    for (fname, doc) in files_to_write {
        let dst_path = docs_dir.join(fname);
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

    fs::write(out_dir.join(PUBLISHED_API_FILE), published_api).unwrap();
}

/// Post process the generated docs so that they are in a format that can be consumed by
/// docusaurus.
/// * Flatten out the tree-like structure of the docs directory that we generate for a package into
///   a flat list of packages;
/// * Deduplicate packages (since multiple packages could share dependencies); and
/// * Write out the package docs in a flat directory structure.
fn relocate_docs(files: &[(String, String)], output: &mut BTreeMap<String, String>) {
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
        if file_name.contains("dependencies") {
            // we don't need to keep the dependency version of each doc since it will be generated
            // on its own
            continue;
        };
        output.entry(file_name.to_owned()).or_insert_with(|| {
            re.replace_all(
                &file_content
                    .replace("../../dependencies/", "../")
                    .replace("../dependencies/", "../")
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
