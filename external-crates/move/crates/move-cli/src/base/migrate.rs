// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package_alt_compilation::build_config::BuildConfig;

use crate::base::find_env;
use move_package_alt::flavor::MoveFlavor;
use std::path::Path;

/// Migrate to Move 2024 for the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "migrate")]
pub struct Migrate;

impl Migrate {
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        config: BuildConfig,
    ) -> anyhow::Result<()> {
        let path = reroot_path(path)?;
        let env = find_env::<F>(&path, &config.clone())?;
        config
            .migrate_package::<F, _, _>(
                &path,
                env,
                &mut std::io::stdout(),
                &mut std::io::stdin().lock(),
            )
            .await?;
        Ok(())
    }
}
