// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use move_package_alt::{RootPackage, Vanilla};
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::Path;
use tempfile::TempDir;

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
