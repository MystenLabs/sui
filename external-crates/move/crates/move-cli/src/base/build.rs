// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::{find_env, reroot_path};
use clap::*;
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::build_config::BuildConfig;
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
        let path = reroot_path(path)?;
        let env = find_env::<F>(&path, &config)?;

        config
            .compile::<F, _>(&path, &env, &mut std::io::stdout())
            .await?;

        Ok(())
    }
}
