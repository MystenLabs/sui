// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use move_stackless_bytecode_2::from_compiled_modules;

use move_core_types::account_address::AccountAddress;
use move_package_alt_compilation::build_plan::BuildPlan;
use move_symbol_pool::Symbol;
use std::{collections::BTreeSet, path::Path};

type ModuleKey = (AccountAddress, Symbol);

fn lib_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let test_module_names = common::read_test_module_names(file_path)?;

    let (root_package_name, compiled_package) =
        common::run_test_package_build(pkg_dir, |writer, root_pkg, config| {
            let canonical_package_name = Symbol::from(root_pkg.package_info().name().as_str());
            let compiled_package = BuildPlan::create(root_pkg, config)?
                .compile_no_exit(writer, |compiler| compiler)?;
            Ok((canonical_package_name, compiled_package))
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
        root_package_name,
        &root_modules,
        &test_module_names,
        "compiled.opt.sbir",
    )?;

    let (_mdl, bytecode) = from_compiled_modules(modules, /* optimize */ false)?;
    assert_modules(
        file_path,
        &bytecode,
        root_package_name,
        &root_modules,
        &test_module_names,
        "compiled.no_opt.sbir",
    )?;

    Ok(())
}

/// Validates test output for input root modules.
fn assert_modules(
    file_path: &Path,
    bytecode: &move_stackless_bytecode_2::ast::StacklessBytecode,
    root_package_name: Symbol,
    root_modules: &BTreeSet<ModuleKey>,
    test_module_names: &BTreeSet<Symbol>,
    suffix: &str,
) -> anyhow::Result<()> {
    // Dependencies are needed for translation but excluded from snapshot output.
    let modules = bytecode.packages.iter().flat_map(|pkg| {
        pkg.modules.values().filter_map(move |module| {
            root_modules
                .contains(&(pkg.address, module.name))
                .then_some((root_package_name, module))
        })
    });
    common::assert_modules(file_path, modules, test_module_names, suffix)
}

datatest_stable::harness!(lib_test, "tests/move", r"from_compiled.txt$");
