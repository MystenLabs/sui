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
pub struct Lint {
    /// Print an explanation for a lint and exit, without building the package. Accepts a lint name
    /// (e.g. `share_owned`) or its code (e.g. `W99000`).
    #[clap(long, value_name = "CODE")]
    pub explain: Option<String>,

    /// List every lint with its group and summary, and exit.
    #[clap(long)]
    pub list: bool,
}

impl Lint {
    pub async fn execute<F: MoveFlavor>(
        self,
        path: Option<&Path>,
        mut config: BuildConfig,
        flavor: F,
    ) -> anyhow::Result<()> {
        if self.list {
            print!("{}", move_compiler::linters::docs::LintIndex);
            return Ok(());
        }

        if let Some(query) = self.explain.as_deref() {
            match move_compiler::linters::docs::find_lint_doc(query) {
                Some(doc) => print!("{doc}"),
                None => {
                    anyhow::bail!("unknown lint `{query}`; run `lint --list` to see every lint")
                }
            }
            return Ok(());
        }

        let rerooted_path = reroot_path(path)?;
        let env = find_env(&rerooted_path, &config, &flavor)?;

        config.lint_flag.set(LintLevel::All);

        config
            .check_package(&rerooted_path, &env, flavor, &mut std::io::stdout())
            .await
    }
}
