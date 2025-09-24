// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::reroot_path;
use clap::*;
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::{build_config::BuildConfig, find_env};
use std::path::Path;

/// Build the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "build")]
pub struct Build;

impl Build {
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        config: BuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let env = find_env::<F>(&rerooted_path, &config)?;

        config
            .compile_package::<F, _>(&rerooted_path, &env, &mut std::io::stdout())
            .await?;

        Ok(())
    }
}
