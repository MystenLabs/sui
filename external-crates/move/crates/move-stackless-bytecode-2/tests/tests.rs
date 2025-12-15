// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::from_model;

use move_command_line_common::insta_assert;
use move_symbol_pool::Symbol;

use tempfile::TempDir;

use move_package_alt::{flavor::Vanilla, package::RootPackage};
use move_package_alt_compilation::{build_config::BuildConfig, model_builder};

use std::{collections::BTreeSet, io::BufRead, path::Path};

fn run_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();
    let output_dir = TempDir::new()?;

    let config = BuildConfig {
        install_dir: Some(output_dir.path().to_path_buf()),
        force_recompilation: false,
        ..Default::default()
    };

    let mut writer = Vec::new();

    // Block on the async function
    let env = move_package_alt::flavor::vanilla::default_environment();
    let root_pkg =
        RootPackage::<Vanilla>::load_sync(pkg_dir.to_path_buf(), env, config.mode_set())?;

    let test_module_names = std::io::BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    let test_module_names = test_module_names
        .into_iter()
        .map(|name| name.into())
        .collect::<BTreeSet<Symbol>>();

    let model = model_builder::build(&mut writer, &root_pkg, &config)?;
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

    let model = model_builder::build(&mut writer, &root_pkg, &config)?;
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
datatest_stable::harness!(run_test, "tests/move", r"modules\.txt$");
