// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::bail;
use clap::{ArgAction, Parser};

use crate::{
    flavor::Vanilla,
    package::RootPackage,
    schema::{Environment, EnvironmentName, ModeName},
};

/// Re-pin the dependencies of this package.
#[derive(Debug, Clone, Parser)]
pub struct UpdateDeps {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,
    /// The environment to update dependencies for. If none is provided, all environments'
    /// dependencies will be updated.
    #[arg(name = "environment", short = 'e', long = "environment")]
    environment: EnvironmentName,

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
    pub async fn execute(&self) -> anyhow::Result<()> {
        let path = self.path.clone().unwrap_or(PathBuf::from("."));

        let envs = RootPackage::<Vanilla>::environments(&path)?;

        let Some(chain_id) = envs.get(&self.environment) else {
            bail!("Environment {} not found", self.environment);
        };

        let environment = Environment::new(self.environment.clone(), chain_id.clone());

        let mut root_package =
            RootPackage::<Vanilla>::load_force_repin(&path, environment, self.modes.clone())
                .await?;
        root_package.save_lockfile_to_disk()?;

        Ok(())
    }
}
