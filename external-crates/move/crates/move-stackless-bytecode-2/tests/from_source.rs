// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::generator::StacklessBytecodeGenerator;

use move_command_line_common::insta_assert;
use move_package_alt_compilation::{build_config::BuildConfig, model_builder};
use move_symbol_pool::Symbol;

use tempfile::TempDir;

use move_package_alt::{flavor::Vanilla, package::RootPackage};
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
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Block on the async function
    let env = move_package_alt::flavor::vanilla::default_environment();
    let root_pkg = rt.block_on(async { RootPackage::<Vanilla>::load(pkg_dir, env).await })?;
    let model = model_builder::build(&mut writer, root_pkg, &config)?;

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
        let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name.expect("NO PACKAGE NAME"), module_name);
                let stackless_bytecode = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: ".opt.sbir",
                };
            }
        }
    }

    let packages = generator.generate_stackless_bytecode(/* optimize */ false)?;

    for pkg in &packages {
        let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name.expect("NO PACKAGE NAME"), module_name);
                let stackless_bytecode = format!("{}", module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: ".no_opt.sbir",
                };
            }
        }
    }

    Ok(())
}

// Hand in each Move.toml path
datatest_stable::harness!(run_test, "tests/move", r"from_source\.txt$");
