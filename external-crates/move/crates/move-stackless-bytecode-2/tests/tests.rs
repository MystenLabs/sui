// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::{generator::StacklessBytecodeGenerator, stackless::ast};

use move_command_line_common::insta_assert;
use move_package::{BuildConfig, compilation::model_builder};
use move_symbol_pool::Symbol;

use tempfile::TempDir;

use std::{collections::BTreeSet, io::BufRead, path::Path};

fn run_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();
    // let toml_path = Path::join(&pkg_dir, "Move.toml");
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

    // let bytecode_files = find_filenames(&[output_dir], |path| {
    //     extension_equals(path, MOVE_COMPILED_EXTENSION)
    // })?;

    let generator = StacklessBytecodeGenerator::from_model(model);

    let test_module_names = std::io::BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;
    let test_module_names = test_module_names
        .into_iter()
        .map(|name| name.into())
        .collect::<BTreeSet<Symbol>>();

    let packages = generator.generate_stackless_bytecode()?;

    for pkg in &packages {
        let pkg_name = pkg.name;
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}::{}", pkg_name, module_name);
                let stackless_bytecode = render(&name, module);
                insta_assert! {
                    input_path: file_path,
                    contents: stackless_bytecode,
                    name: name,
                    suffix: ".sbir",
                };
            }
        }
    }

    Ok(())
}

fn render(name: &str, module: &ast::Module) -> String {
    use std::io::Write;
    let mut writer = Vec::new();
    // TODO: Write a display instance for a module and use it here instead
    writeln!(writer, "Module {}", name).unwrap();

    for (name, fun) in &module.functions {
        writeln!(writer, "    Function {}", name).unwrap();
        for instr in &fun.instructions {
            writeln!(writer, "        {}", instr).unwrap();
        }
    }
    String::from_utf8(writer).expect("Output is not valid UTF-8")
}

// Hand in each Move.toml path
datatest_stable::harness!(run_test, "tests/move", r"modules\.txt$");
