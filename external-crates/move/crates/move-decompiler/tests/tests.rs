// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_decompiler::decompile_module;

use move_command_line_common::insta_assert;
use move_package::{BuildConfig, compilation::model_builder};
use move_stackless_bytecode_2::generator::StacklessBytecodeGenerator;
use move_symbol_pool::Symbol;

use tempfile::TempDir;

use std::{collections::BTreeSet, io::BufRead, path::Path};

// -------------------------------------------------------------------------------------------------
// Structuring Unit Tests

fn run_structuring_test(file_path: &Path) -> datatest_stable::Result<()> {
    let name = file_path.file_stem().unwrap().to_str().unwrap().to_owned();
    let result = move_decompiler::structuring_unit_test(file_path);
    insta_assert! {
        input_path: file_path,
        contents: result,
        name: name,
    };
    Ok(())
}

// -------------------------------------------------------------------------------------------------
// Move Tests [disabled for now]

#[allow(dead_code)]
fn run_move_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();
    let output_dir = TempDir::new()?;

    let config = BuildConfig {
        dev_mode: true,
        install_dir: Some(output_dir.path().to_path_buf()),
        force_recompilation: false,
        ..Default::default()
    };

    let mut writer = Vec::new();
    let resolved_package = config.resolution_graph_for_package(pkg_dir, None, &mut writer)?;
    let model = model_builder::build(resolved_package, &mut writer)?;

    let generator = StacklessBytecodeGenerator::from_model(model);

    let test_module_names = std::io::BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    let test_module_names = test_module_names
        .into_iter()
        .map(|name| name.into())
        .collect::<BTreeSet<Symbol>>();

    let packages = generator.generate_stackless_bytecode(/* optimize */ true)?;

    for pkg in &packages {
        // let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                // FIXME pkg name not coherent, address name returned instead let name = format!("{}_{}", pkg_name.expect("NO PACKAGE NAME"), module_name);
                let name = format!("{}", module_name);
                let module = decompile_module(module.clone());
                let decompiled = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: decompiled,
                    name: name,
                };
            }
        }
    }

    Ok(())
}

// Hand in each move path
datatest_stable::harness!(
    run_move_test,
    "tests/move",
    r"modules\.txt$",
    run_structuring_test,
    "tests/structuring",
    r"\.stt$"
);
