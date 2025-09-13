// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::from_compiled_modules;

use move_command_line_common::insta_assert;
use move_symbol_pool::Symbol;
use std::{collections::BTreeSet, io::BufRead, path::Path};

use move_binary_format::CompiledModule;
use move_command_line_common::files::{MOVE_COMPILED_EXTENSION, extension_equals, find_filenames};

fn lib_test(file_path: &Path) -> datatest_stable::Result<()> {
    let pkg_dir = file_path.parent().unwrap();

    let bytecode_files = find_filenames(&[pkg_dir], |path| {
        extension_equals(path, MOVE_COMPILED_EXTENSION)
    })
    .expect("Failed to find bytecode files");

    let mut modules = Vec::new();

    let test_module_names = std::io::BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?;

    if test_module_names.contains(&String::from("skip")) {
        return Ok(());
    }

    for bytecode_file in &bytecode_files {
        let bytes = std::fs::read(bytecode_file)?;
        let module = CompiledModule::deserialize_with_defaults(&bytes)?;
        modules.push(module);
    }

    let (_mdl, bytecode) = from_compiled_modules(modules.clone(), /* optimize */ true)?;

    let test_module_names = test_module_names
        .into_iter()
        .map(|name| name.into())
        .collect::<BTreeSet<Symbol>>();

    for pkg in &bytecode.packages {
        let pkg_name = pkg
            .name
            .unwrap_or(Symbol::from(pkg.address.to_hex_literal()));
        // let pkg_path = file_path.join(pkg_name.to_string());
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name, module_name);
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

    let (_mdl, bytecode) = from_compiled_modules(modules, /* optimize */ false)?;

    for pkg in &bytecode.packages {
        let pkg_name = pkg
            .name
            .unwrap_or(Symbol::from(pkg.address.to_hex_literal()));
        for (module_name, module) in &pkg.modules {
            if test_module_names.contains(module_name) {
                let name = format!("{}_{}", pkg_name, module_name);
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

datatest_stable::harness!(lib_test, "tests/move", r"from_bytecode.txt$");
