// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_package_alt::{
    flavor::{MoveFlavor, Vanilla},
    package::RootPackage,
    schema::Environment,
};
use move_package_alt_compilation::compile_package;
use move_package_alt_compilation::{build_config::BuildConfig, build_plan::BuildPlan};
use std::{
    io::Stdout,
    path::{Path, PathBuf},
};

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
        let p = PathBuf::from(".");
        let path = path.clone().unwrap_or_else(|| &p);

        let envs = RootPackage::<Vanilla>::environments(path)?;

        let env = if let Some(ref e) = config.environment {
            if let Some(env) = envs.get(e) {
                Environment::new(e.to_string(), env.to_string())
            } else {
                let (name, id) = envs.first_key_value().expect("At least one default env");
                Environment::new(name.to_string(), id.to_string())
            }
        } else {
            let (name, id) = envs.first_key_value().expect("At least one default env");
            Environment::new(name.to_string(), id.to_string())
        };

        let root_pkg = RootPackage::<F>::load(path, env).await?;

        let mut build_plan = BuildPlan::create(root_pkg, &config)?;
        let compiled_package = build_plan.compile(&mut std::io::stdout(), |compiler| compiler)?;

        Ok(())
    }
}
