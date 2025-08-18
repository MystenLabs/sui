// Copyright (c) The Diem Core Contributors
//
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// A canonical path to a directory containing a loaded Move package (in particular, the directory
/// must have a Move.toml)
#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct PackagePath(PathBuf);

#[derive(Error, Debug)]
pub enum PackagePathError {
    #[error("Invalid directory at `{path}`")]
    InvalidDirectory { path: PathBuf },

    #[error("Package does not have a Move.toml file at `{path_to_toml}`")]
    InvalidPackage { path_to_toml: PathBuf },
}

pub type PackagePathResult<T> = Result<T, PackagePathError>;

impl PackagePath {
    /// Create a canonical path from the given [`dir`]. This function checks that there is a
    /// directory at `dir` and that it contains a valid Move package, i.e., it has a `Move.toml`
    /// file.
    pub fn new(dir: PathBuf) -> PackagePathResult<Self> {
        let path = dir
            .canonicalize()
            .map_err(|e| PackagePathError::InvalidDirectory { path: dir.clone() })?;

        if !dir.is_dir() {
            return Err(PackagePathError::InvalidDirectory { path: dir.clone() });
        }

        let result = Self(path);

        if !result.manifest_path().exists() {
            return Err(PackagePathError::InvalidPackage {
                path_to_toml: result.manifest_path(),
            });
        }

        Ok(result)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The path to the Move.toml file in this package.
    pub fn manifest_path(&self) -> PathBuf {
        self.0.join("Move.toml")
    }

    /// The path to the lockfile
    pub fn lockfile_path(&self) -> PathBuf {
        self.0.join("Move.lock")
    }

    /// The path to the publications file
    pub fn publications_path(&self) -> PathBuf {
        self.0.join("Move.published")
    }

    /// The path to the local publications file
    pub fn publications_local_path(&self) -> PathBuf {
        self.0.join("Move.published.local")
    }
}

impl AsRef<Path> for PackagePath {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

#[cfg(test)]
mod tests {
    use move_command_line_common::testing::insta::assert_snapshot;

    use super::*;
    use std::fs;

    /// ensures that [PackagePath::path] returns a canonicalized path
    #[test]
    fn test_canonicalization() {
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

    /// You must create a package path from the directory containing the `Move.toml` file, rather
    /// than the path to `Move.toml` itself
    #[test]
    fn test_move_toml_path_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manifest_path = temp_dir.path().join("Move.toml");

        // Create a Move.toml file in the temporary directory
        fs::write(&manifest_path, "[package]\nname = 'test_package'\n").unwrap();

        assert_snapshot!(PackagePath::new(manifest_path).unwrap_err().to_string(), @"");
    }
}
