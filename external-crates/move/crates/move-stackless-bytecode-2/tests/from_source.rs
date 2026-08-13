// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod common;

use move_stackless_bytecode_2::from_model;

use move_command_line_common::insta_assert;
use move_package_alt_compilation::model_builder;

use std::path::Path;

fn run_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let test_module_names = common::read_test_module_names(file_path)?;

    let model = common::run_test_package_build(pkg_dir, model_builder::build)?;
    let bytecode = from_model(&model, /* optimize */ true)?;

    for pkg in &bytecode.packages {
        let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name.expect("NO PACKAGE NAME"), module_name);
                let stackless_bytecode = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: "opt.sbir",
                };
            }
        }
    }

    let bytecode = from_model(&model, /* optimize */ false)?;

    for pkg in &bytecode.packages {
        let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name.expect("NO PACKAGE NAME"), module_name);
                let stackless_bytecode = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: "no_opt.sbir",
                };
            }
        }
    }

    Ok(())
}

// Hand in each Move.toml path
datatest_stable::harness!(run_test, "tests/move", r"from_source\.txt$");
