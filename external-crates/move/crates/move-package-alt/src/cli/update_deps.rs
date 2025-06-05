// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    dependency::pin,
    errors::{PackageError, PackageResult},
    flavor::Vanilla,
    graph::PackageGraph,
    package::{Package, RootPackage, lockfile::Lockfile, manifest::Manifest, paths::PackagePath},
};
use clap::{Command, Parser, Subcommand};

/// Re-pin the dependencies of this package.
#[derive(Debug, Clone, Parser)]
pub struct UpdateDeps {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,
    /// The environment to update dependencies for. If none is provided, all environments'
    /// dependencies will be updated.
    #[arg(name = "environment", short = 'e', long = "environment")]
    environment: Option<String>,
}

impl UpdateDeps {
    pub async fn execute(&self) -> PackageResult<()> {
        let mut root_package =
            RootPackage::<Vanilla>::load(self.path.as_ref().unwrap_or(&PathBuf::from("."))).await?;
        let envs = if let Some(env) = &self.environment {
            let envs = root_package
                .environments()
                .into_iter()
                .filter(|e| e.0 == env)
                .map(|e| (e.0.clone(), e.1.clone()))
                .collect::<BTreeMap<_, _>>();

            Some(envs)
        } else {
            None
        };

        root_package.repin(envs.clone()).await?;
        root_package.serialize_lockfile(envs).await?;

        Ok(())
    }
}
