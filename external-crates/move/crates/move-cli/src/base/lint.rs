// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::reroot_path;
use clap::*;
use move_compiler::linters::LintLevel;
use move_package_alt::{MoveFlavor, RootPackage};
use move_package_alt_compilation::{
    build_config::BuildConfig, compilation::build_for_driver, find_env,
};
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
    ) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let env = find_env::<F>(&rerooted_path, &config)?;

        config.lint_flag.set(LintLevel::All);

        let root_pkg: RootPackage<F> = config.package_loader(&rerooted_path, &env).load().await?;
        let dependencies = root_pkg
            .packages()
            .into_iter()
            .filter(|x| !x.is_root())
            .map(|x| x.id().to_string())
            .collect();

        build_for_driver::<_, _, F>(
            &mut std::io::stdout(),
            None,
            &config,
            &root_pkg,
            dependencies,
            |compiler| {
                compiler.check_and_report()?;
                Ok(())
            },
        )?;

        Ok(())
    }
}
