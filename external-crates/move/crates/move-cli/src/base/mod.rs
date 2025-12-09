// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod build;
pub mod coverage;
pub mod decompile;
pub mod disassemble;
pub mod docgen;
pub mod migrate;
pub mod new;
pub mod profile;
pub mod summary;
pub mod test;
pub mod update_deps;

use move_package_alt::package::layout::SourcePackageLayout;
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
