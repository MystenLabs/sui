// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    flavor::Vanilla,
    package::RootPackage,
    schema::{Environment, EnvironmentName, ModeName},
};
use anyhow::bail;
use clap::{ArgAction, Parser};

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Build {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,
    /// The environment to build for. If none is provided, all environments'
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

impl Build {
    pub async fn execute(&self) -> anyhow::Result<()> {
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));

        let envs = RootPackage::<Vanilla>::environments(&path)?;

        let Some(chain_id) = envs.get(&self.environment) else {
            bail!("Environment {} not found", self.environment);
        };

        let environment = Environment::new(self.environment.clone(), chain_id.clone());

        let mut root_pkg =
            RootPackage::<Vanilla>::load(&path, environment, self.modes.clone()).await?;

        for pkg in root_pkg.packages() {
            println!("Package {}", pkg.name());
            if pkg.is_root() {
                println!("  (root package)");
            }
            println!("  path: {:?}", pkg.path());
            println!("  named addresses:");
            for (name, addr) in pkg.named_addresses()? {
                println!("    {name}: {addr:?}");
            }
            println!();
        }

        root_pkg.save_lockfile_to_disk()?;
        Ok(())
    }
}
