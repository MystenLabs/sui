// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::{ast::StacklessBytecode, from_compiled_modules};

use anyhow::{Context, ensure};
use move_command_line_common::insta_assert;
use move_core_types::account_address::AccountAddress;
use move_package_alt::{RootPackage, Vanilla};
use move_package_alt_compilation::{build_config::BuildConfig, build_plan::BuildPlan};
use move_symbol_pool::Symbol;
use std::{collections::BTreeSet, io::BufRead, path::Path};
use tempfile::TempDir;

type ModuleKey = (AccountAddress, Symbol);

fn lib_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let test_module_names = std::io::BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    let test_module_names = test_module_names
        .into_iter()
        .map(|name| name.into())
        .collect::<BTreeSet<Symbol>>();
    if test_module_names.is_empty() {
        return Err(anyhow::anyhow!("no modules requested by {}", file_path.display()).into());
    }

    let output_dir = TempDir::new()?;
    let config = BuildConfig {
        install_dir: Some(output_dir.path().to_path_buf()),
        ..Default::default()
    };
    let env = Vanilla::default_environment();
    let root_pkg: RootPackage<Vanilla> = config
        .package_loader(pkg_dir, &env, Vanilla::new())
        .load_sync()?;
    let mut writer = Vec::new();
    let compiled_package = BuildPlan::create(&root_pkg, &config)?
        .compile_no_exit(&mut writer, |compiler| compiler)
        .with_context(|| {
            format!(
                "failed to compile {}:\n{}",
                pkg_dir.display(),
                String::from_utf8_lossy(&writer)
            )
        })?;

    let root_modules = compiled_package
        .root_modules()
        .map(|unit| {
            let module = &unit.unit.module;
            (*module.address(), Symbol::from(module.name().as_str()))
        })
        .collect::<BTreeSet<_>>();
    if root_modules.is_empty() {
        return Err(anyhow::anyhow!(
            "compilation produced no root modules for {}",
            pkg_dir.display()
        )
        .into());
    }

    let modules = compiled_package
        .get_modules_and_deps()
        .cloned()
        .collect::<Vec<_>>();

    let (_mdl, bytecode) = from_compiled_modules(modules.clone(), /* optimize */ true)?;
    assert_modules(
        file_path,
        &bytecode,
        &root_modules,
        &test_module_names,
        "bytecode.opt.sbir",
    )?;

    let (_mdl, bytecode) = from_compiled_modules(modules, /* optimize */ false)?;
    assert_modules(
        file_path,
        &bytecode,
        &root_modules,
        &test_module_names,
        "bytecode.no_opt.sbir",
    )?;

    Ok(())
}

/// For each module name listed in `from_bytecode.txt`, finds the corresponding translated root
/// module and compares its formatted output with the module's snapshot file. Fails if any listed
/// name does not correspond to a translated root module.
fn assert_modules(
    file_path: &Path,
    bytecode: &StacklessBytecode,
    root_modules: &BTreeSet<ModuleKey>,
    test_module_names: &BTreeSet<Symbol>,
    suffix: &str,
) -> anyhow::Result<()> {
    let mut observed_modules = BTreeSet::new();

    for pkg in &bytecode.packages {
        let pkg_name = pkg
            .name
            .unwrap_or(Symbol::from(pkg.address.to_hex_literal()));
        for (module_name, module) in &pkg.modules {
            let module_key = (pkg.address, *module_name);
            if root_modules.contains(&module_key) && test_module_names.contains(module_name) {
                observed_modules.insert(*module_name);
                let name = format!("{}_{}", pkg_name, module_name);
                let stackless_bytecode = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: suffix,
                };
            }
        }
    }

    ensure!(
        &observed_modules == test_module_names,
        "requested modules were not translated for {suffix}: {}",
        test_module_names
            .difference(&observed_modules)
            .map(Symbol::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );

    Ok(())
}

datatest_stable::harness!(lib_test, "tests/move", r"from_bytecode.txt$");
