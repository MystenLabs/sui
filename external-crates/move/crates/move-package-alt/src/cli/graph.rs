// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    errors::PackageResult,
    flavor::Vanilla,
    graph::PackageGraph,
    package::{EnvironmentName, Package, paths::PackagePath},
};
use clap::{Command, Parser, Subcommand};
use petgraph::dot::{Config, Dot};
use tracing::info;

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Graph {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,
}

impl Graph {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let path = path.canonicalize().unwrap();
        let package_path = PackagePath::new(path.clone())?;

        let graph = PackageGraph::<Vanilla>::load(&package_path).await?;

        println!("Package graph: {:?}", graph);

        Ok(())
    }
}
