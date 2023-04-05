// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package::{Architecture, BuildConfig};
use std::path::PathBuf;

/// Build the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "build")]
pub struct Build;

impl Build {
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        if config.fetch_deps_only {
            let mut config = config;
            if config.test_mode {
                config.dev_mode = true;
            }
            config.download_deps_for_package(&rerooted_path, &mut std::io::stdout())?;
            return Ok(());
        }
        let architecture = config.architecture.unwrap_or(Architecture::Move);

        match architecture {
            Architecture::Move | Architecture::AsyncMove => {
                config.compile_package(&rerooted_path, &mut std::io::stdout())?;
            }

            Architecture::Ethereum => {
                #[cfg(feature = "evm-backend")]
                config.compile_package_evm(&rerooted_path, &mut std::io::stderr())?;

                #[cfg(not(feature = "evm-backend"))]
                anyhow::bail!("The Ethereum architecture is not supported because move-cli was not compiled with feature flag `evm-backend`.");
            }
        }
        Ok(())
    }
}
