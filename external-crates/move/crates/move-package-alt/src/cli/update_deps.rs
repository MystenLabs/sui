// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    dependency::pin,
    errors::{PackageError, PackageResult},
    flavor::Vanilla,
    graph::PackageGraph,
    package::{Package, lockfile::Lockfile, manifest::Manifest, paths::PackagePath},
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
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        let package = Package::<Vanilla>::load_root(&path).await?;
        let pkg_path = package.path();
        let envs = if let Some(ref env) = self.environment {
            let envs = package
                .manifest()
                .environments()
                .iter()
                .find(|(e, _)| *e == env)
                .ok_or_else(|| PackageError::Generic("Environment not found".to_string()))?;
            &BTreeMap::from([(envs.0.clone(), envs.1.clone())])
        } else {
            package.manifest().environments()
        };

        let mut lockfiles = Lockfile::<Vanilla>::read_from_dir(&pkg_path.path())?;

        for env in envs.keys() {
            let pkg_graph = PackageGraph::<Vanilla>::load_from_manifests(pkg_path, env).await?;
            let updated_pinned_deps = pkg_graph.to_pinned_deps(pkg_path, env).await?;
            lockfiles.update_pinned_dep_env(updated_pinned_deps);
        }

        lockfiles.write_to(&pkg_path.path(), envs.clone())?;

        Ok(())
    }
}
