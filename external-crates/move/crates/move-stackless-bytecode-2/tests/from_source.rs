// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use move_stackless_bytecode_2::from_model;

use move_package_alt_compilation::model_builder;

use std::path::Path;

fn run_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let test_module_names = common::read_test_module_names(file_path)?;

    let model = common::run_test_package_build(pkg_dir, model_builder::build)?;
    let bytecode = from_model(&model, /* optimize */ true)?;
    assert_modules(file_path, &bytecode, &test_module_names, "opt.sbir")?;

    let bytecode = from_model(&model, /* optimize */ false)?;
    assert_modules(file_path, &bytecode, &test_module_names, "no_opt.sbir")?;

    Ok(())
}

/// Validates test output for input modules from all translated packages.
fn assert_modules(
    file_path: &Path,
    bytecode: &move_stackless_bytecode_2::ast::StacklessBytecode,
    test_module_names: &std::collections::BTreeSet<move_symbol_pool::Symbol>,
    suffix: &str,
) -> anyhow::Result<()> {
    let modules = bytecode.packages.iter().flat_map(|pkg| {
        let package_name = pkg.name.expect("NO PACKAGE NAME");
        pkg.modules
            .values()
            .map(move |module| (package_name, module))
    });
    common::assert_modules(file_path, modules, test_module_names, suffix)
}

// Hand in each Move.toml path
datatest_stable::harness!(run_test, "tests/move", r"from_source\.txt$");
