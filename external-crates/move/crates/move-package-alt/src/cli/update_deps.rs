// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::Parser;

use crate::{
    errors::{PackageError, PackageResult},
    flavor::Vanilla,
    package::RootPackage,
    schema::{Environment, EnvironmentName},
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
}

impl UpdateDeps {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = self.path.clone().unwrap_or(PathBuf::from("."));

        let envs = RootPackage::<Vanilla>::environments(&path)?;

        let Some(chain_id) = envs.get(&self.environment) else {
            return Err(PackageError::Generic(format!(
                "Environment {} not found",
                self.environment
            )));
        };

        let environment = Environment::new(self.environment.clone(), chain_id.clone());

        let root_package = RootPackage::<Vanilla>::load_force_repin(&path, environment).await?;
        root_package.save_to_disk()?;

        Ok(())
    }
}
