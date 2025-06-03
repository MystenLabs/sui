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
}

impl UpdateDeps {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = PackagePath::new(self.path.clone().unwrap_or_else(|| PathBuf::from(".")))?;
        let manifest = Manifest::<Vanilla>::read_from_file(&path.manifest_path())?;

        for (env, id) in manifest.environments() {
            let package_graph = PackageGraph::<Vanilla>::load_from_manifests(&path, env).await?;

            package_graph.to_lockfile(&path, env).await?;
        }

        Ok(())
    }
}
