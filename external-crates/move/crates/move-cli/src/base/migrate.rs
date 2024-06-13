// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package::BuildConfig;
use std::path::Path;

/// Migrate to Move 2024 for the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "migrate")]
pub struct Migrate;

impl Migrate {
    pub fn execute(self, path: Option<&Path>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        config.migrate_package(
            &rerooted_path,
            &mut std::io::stdout(),
            &mut std::io::stdin().lock(),
        )?;
        Ok(())
    }
}
