// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use move_stackless_bytecode_2::{ast::StacklessBytecode, from_compiled_modules};

use anyhow::ensure;
use move_command_line_common::insta_assert;
use move_core_types::account_address::AccountAddress;
use move_package_alt_compilation::build_plan::BuildPlan;
use move_symbol_pool::Symbol;
use std::{collections::BTreeSet, path::Path};

type ModuleKey = (AccountAddress, Symbol);

fn lib_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let test_module_names = common::read_test_module_names(file_path)?;

    let compiled_package = common::run_test_package_build(pkg_dir, |writer, root_pkg, config| {
        BuildPlan::create(root_pkg, config)?.compile_no_exit(writer, |compiler| compiler)
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
        "compiled.opt.sbir",
    )?;

    let (_mdl, bytecode) = from_compiled_modules(modules, /* optimize */ false)?;
    assert_modules(
        file_path,
        &bytecode,
        &root_modules,
        &test_module_names,
        "compiled.no_opt.sbir",
    )?;

    Ok(())
}

/// For each module name listed in `from_compiled.txt`, finds the corresponding translated root
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

datatest_stable::harness!(lib_test, "tests/move", r"from_compiled.txt$");
