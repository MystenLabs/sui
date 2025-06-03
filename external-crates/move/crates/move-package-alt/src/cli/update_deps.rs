// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    dependency::pin,
    errors::PackageResult,
    flavor::Vanilla,
    graph::PackageGraph,
    package::{Package, manifest::Manifest, paths::PackagePath},
};
use clap::{Command, Parser, Subcommand};

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct UpdateDeps {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,

    #[arg(name = "environment", short = 'e', long = "environment")]
    environment: Option<String>,
}

impl UpdateDeps {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = PackagePath::new(self.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;

        let package = Package::<Vanilla>::load_root(&path.path()).await?;
        let envs = package.manifest().environments();

        // update the lockfile
        // let package_graph = PackageGraph::<Vanilla>::load(&path, envs.clone()).await?;

        Ok(())
    }
}
