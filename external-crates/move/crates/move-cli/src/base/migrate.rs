// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::{
    io::{self, BufRead, BufReader},
    path::Path,
};

use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::{build_config::BuildConfig, find_env};

use super::reroot_path;

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
        let rerooted_path = reroot_path(path)?;
        let env = find_env::<F>(&rerooted_path, &config.clone())?;
        let mut reader: Box<dyn BufRead + Send> = Box::new(BufReader::new(io::stdin()));
        config
            .migrate_package::<F, _, _>(&rerooted_path, env, &mut io::stdout(), &mut reader)
            .await?;
        Ok(())
    }
}
