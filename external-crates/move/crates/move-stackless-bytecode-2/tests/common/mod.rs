// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use move_package_alt::{RootPackage, Vanilla};
use move_package_alt_compilation::build_config::BuildConfig;
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
