// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::bail;
use clap::{ArgAction, Parser};

use move_package_alt::{
    flavor::MoveFlavor,
    package::RootPackage,
    schema::{Environment, EnvironmentName, ModeName},
};

/// Re-pin the dependencies of this package.
#[derive(Debug, Clone, Parser)]
pub struct UpdateDeps {
    /// The environment to update dependencies for. If none is provided, all environments'
    /// dependencies will be updated.
    #[arg(name = "environment", short = 'e', long = "environment")]
    environment: Option<EnvironmentName>,
    /// Arbitrary mode -- this will be used to enable or filter user-defined `#[mode(<MODE>)]`
    /// annodations during compiltaion.
    #[arg(
        long = "mode",
        value_name = "MODE",
        action = ArgAction::Append,
    )]
    modes: Vec<ModeName>,
}

impl UpdateDeps {
    pub async fn execute<F: MoveFlavor>(&self, path: Option<&Path>) -> anyhow::Result<()> {
        let default = PathBuf::from(".");
        let path = path.unwrap_or(&default);
        let mut envs = RootPackage::<F>::environments(&path)?;

        if let Some(env) = self.environment.as_ref() {
            let Some(_) = envs.get(env) else {
                bail!("Environment {} not found", env);
            };
            envs.retain(|env_name, _| env_name == env);
        }

        for e in envs {
            let env = Environment::new(e.0.clone(), e.1.clone());
            let mut root_package =
                RootPackage::<F>::load_force_repin(&path, env, self.modes.clone()).await?;
            root_package.save_lockfile_to_disk()?;
        }

        Ok(())
    }
}
