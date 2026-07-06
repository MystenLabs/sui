// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::reroot_path;
use clap::*;
use move_compiler::linters::LintLevel;
use move_package_alt::MoveFlavor;
use move_package_alt_compilation::{build_config::BuildConfig, find_env};
use std::path::Path;

/// Run Move linters on the package at `path`. If no path is provided defaults to current directory.
#[derive(Parser)]
#[clap(name = "lint")]
pub struct Lint;

impl Lint {
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        mut config: BuildConfig,
        flavor: F,
    ) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let env = find_env(&rerooted_path, &config, &flavor)?;

        config.lint_flag.set(LintLevel::All);

        config
            .check_package(&rerooted_path, &env, flavor, &mut std::io::stdout())
            .await
    }
}
