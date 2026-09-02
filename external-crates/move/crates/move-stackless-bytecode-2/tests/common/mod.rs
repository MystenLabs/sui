// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use move_command_line_common::insta_assert;
use move_package_alt::{RootPackage, Vanilla};
use move_package_alt_compilation::build_config::BuildConfig;
use move_stackless_bytecode_2::ast::Module;
use move_symbol_pool::Symbol;
use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader},
    path::Path,
};
use tempfile::TempDir;

/// Reads module names from `file_path`, trimming whitespace and ignoring blank lines. Fails if the
/// file contains no module names.
pub fn read_test_module_names(file_path: &Path) -> anyhow::Result<BTreeSet<Symbol>> {
    let module_names = BufReader::new(std::fs::File::open(file_path)?)
        .lines()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .map(Symbol::from)
        .collect::<BTreeSet<_>>();

    anyhow::ensure!(
        !module_names.is_empty(),
        "no modules requested by {}",
        file_path.display()
    );

    Ok(module_names)
}

/// Checks that every module named in `selected_module_names` is present in `modules` and that its
/// formatted output matches the existing snapshot selected by `file_path`, its package and module
/// names, and `suffix`.
pub fn assert_modules<'a>(
    file_path: &Path,
    modules: impl IntoIterator<Item = (Symbol, &'a Module)>,
    selected_module_names: &BTreeSet<Symbol>,
    suffix: &str,
) -> anyhow::Result<()> {
    let mut observed_modules = BTreeSet::new();

    for (package_name, module) in modules {
        if selected_module_names.contains(&module.name) {
            observed_modules.insert(module.name);
            let name = format!("{}_{}", package_name, module.name);
            let stackless_bytecode = format!("{}", module);
            insta_assert! {
                input_path: file_path,
                contents: stackless_bytecode,
                name: name,
                suffix: suffix,
            };
        }
    }

    anyhow::ensure!(
        &observed_modules == selected_module_names,
        "requested modules were not translated for {suffix}: {}",
        selected_module_names
            .difference(&observed_modules)
            .map(Symbol::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );

    Ok(())
}

/// Loads `package_dir` with an isolated temporary output directory and runs `build`, attaching
/// captured compiler diagnostics to any build failure.
pub fn run_test_package_build<T>(
    package_dir: &Path,
    build: impl FnOnce(&mut Vec<u8>, &RootPackage<Vanilla>, &BuildConfig) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let output_dir = TempDir::new()?;
    let config = BuildConfig {
        install_dir: Some(output_dir.path().to_path_buf()),
        force_recompilation: false,
        ..Default::default()
    };
    let env = Vanilla::default_environment();
    let root_pkg: RootPackage<Vanilla> = config
        .package_loader(package_dir, &env, Vanilla::new())
        .load_sync()?;
    let mut writer = Vec::new();

    build(&mut writer, &root_pkg, &config).with_context(|| {
        format!(
            "failed to build {}:\n{}",
            package_dir.display(),
            String::from_utf8_lossy(&writer)
        )
    })
}
