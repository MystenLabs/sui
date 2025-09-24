// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod build;
pub mod coverage;
pub mod disassemble;
pub mod docgen;
pub mod migrate;
pub mod new;
pub mod profile;
pub mod summary;
pub mod test;

use anyhow::bail;
use move_package_alt::{
    flavor::MoveFlavor,
    package::{RootPackage, layout::SourcePackageLayout},
    schema::Environment,
};
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::{Path, PathBuf};

/// Reroot the path if none is given
pub fn reroot_path(path: Option<&Path>) -> anyhow::Result<PathBuf> {
    let path = path
        .map(Path::canonicalize)
        .unwrap_or_else(|| PathBuf::from(".").canonicalize())?;
    // Always root ourselves to the package root, and then compile relative to that.
    let rooted_path = SourcePackageLayout::try_find_root(&path)?;
    std::env::set_current_dir(&rooted_path).unwrap();

    Ok(PathBuf::from("."))
}

/// If no environment is passed, it will use the default implicit environment. If an environment
/// is passed, it will try to find it in the list of available environments, and error if it cannot
/// be found.
pub fn find_env<F: MoveFlavor>(path: &Path, config: &BuildConfig) -> anyhow::Result<Environment> {
    let envs = RootPackage::<F>::environments(path)?;
    let env = if let Some(ref e) = config.environment {
        if let Some(env) = envs.get(e) {
            Environment::new(e.to_string(), env.to_string())
        } else {
            bail!(
                "Cannot find environment '{}'. Available environments: {}",
                e,
                envs.keys()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    } else {
        let (name, id) = envs.first().expect("At least one default env");
        Environment::new(name.to_string(), id.to_string())
    };

    Ok(env)
}
