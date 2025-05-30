// Copyright (c) The Diem Core Contributors
//
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::errors::PackageResult;

use std::{
    fmt::{self, Debug, Display},
    path::{Path, PathBuf},
};

use thiserror::Error;

pub type PackagePathResult<T> = Result<T, PackagePathError>;

/// A canonical path to a directory containing a loaded Move package (in particular, the directory
/// must have a Move.toml)
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct PackagePath(PathBuf);

impl PackagePath {
    /// Create a canonical path from the given [`dir`]. This function checks that there is a
    /// `Move.toml` file in this directory.
    pub fn new(dir: PathBuf) -> PackagePathResult<Self> {
        let path = dir
            .canonicalize()
            .map_err(|e| PackagePathError::InvalidDirectory { path: dir.clone() })?;

        if !dir.is_dir() {
            return Err(PackagePathError::InvalidDirectory { path: dir.clone() });
        }

        if !dir.join("Move.toml").exists() {
            return Err(PackagePathError::InvalidPackage { path: dir.clone() });
        }

        Ok(Self(path))
    }

    /// Return the path as a PathBuf
    pub fn path(&self) -> &PathBuf {
        &self.0
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.0.join("Move.toml")
    }
}

#[derive(Error, Debug)]
pub enum PackagePathError {
    #[error("Invalid directory at `{path}`")]
    InvalidDirectory { path: PathBuf },

    #[error("Package does not have a Move.toml file at `{path}`")]
    InvalidPackage { path: PathBuf },
}

pub struct IoError {}

impl Debug for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let relative_path = self.0.strip_prefix(&cwd).unwrap_or(&self.0);
        write!(f, "PackagePath: {}", relative_path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_new() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create the directory structure: temp_dir/A/B/C
        let a = temp_dir.path().join("A");
        fs::create_dir_all(&a).unwrap();

        let b = a.join("B");
        fs::create_dir_all(&b).unwrap();
        let manifest_path = b.join("Move.toml");
        // Create a Move.toml file in the temporary directory
        fs::write(&manifest_path, "[package]\nname = 'test_package'\n").unwrap();

        let c = b.join("C");
        fs::create_dir_all(&c).unwrap();

        // Test going from C to B using relative path
        let relative_path = PathBuf::from("../../B");
        let package_path = PackagePath::new(c.join(relative_path)).unwrap();

        // The result should be the canonicalized path to B
        assert_eq!(package_path.path(), &b.canonicalize().unwrap());
    }

    #[test]
    fn test_move_toml_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manifest_path = temp_dir.path().join("Move.toml");

        // Create a Move.toml file in the temporary directory
        fs::write(&manifest_path, "[package]\nname = 'test_package'\n").unwrap();

        assert!(PackagePath::new(manifest_path).is_err());
    }
}
