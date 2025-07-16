// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    errors::{PackageError, PackageResult},
    flavor::Vanilla,
    package::RootPackage,
    schema::{Environment, EnvironmentName},
};
use clap::Parser;

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
}

impl Build {
    pub async fn execute(&self) -> PackageResult<()> {
        // TODO: consider moving this to move-compilation crate (but I think it's better just to
        // print out what would happen)
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));

        let envs = RootPackage::<Vanilla>::environments(&path)?;

        let Some(chain_id) = envs.get(&self.environment) else {
            return Err(PackageError::Generic(format!(
                "Environment {} not found",
                self.environment
            )));
        };

        let environment = Environment::new(self.environment.clone(), chain_id.clone());

        let root_pkg = RootPackage::<Vanilla>::load(&path, environment).await?;

        // TODO: continue

        // TODO: Implement the actual build logic here.

        root_pkg.save_to_disk();
        Ok(())
    }
}
