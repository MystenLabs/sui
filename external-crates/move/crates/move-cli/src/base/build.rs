// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package::BuildConfig;
use std::path::Path;

/// Build the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "build")]
pub struct Build;

impl Build {
    pub fn execute(self, path: Option<&Path>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        if config.fetch_deps_only {
            let mut config = config;
            if config.test_mode {
                config.dev_mode = true;
            }
            config.download_deps_for_package(&rerooted_path, &mut std::io::stdout())?;
            return Ok(());
        }

        config.clone().cli_compile_package(
            &rerooted_path,
            &mut std::io::stdout(),
            &mut std::io::stdin().lock(),
        )?;
        Ok(())
    }
}
