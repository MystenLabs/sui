// Copyright (c) The Diem Core Contributors
//
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::errors::PackageResult;
use std::{
    fmt::{self, Debug, Display},
    path::{Path, PathBuf},
};

/// An absolute path to a directory containing a loaded Move package (in particular, the directory
/// must have a Move.toml)
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct PackagePath(PathBuf);

impl PackagePath {
    pub fn new(path: PathBuf) -> PackageResult<Self> {
        Ok(Self(path))
    }

    /// Create a new package path from a base path and a relative path. The resulting path is
    /// guaranteed to be absolute and valid, if it exists. Any symbolic links in the path are
    /// resolved to their canonical form.
    pub fn new_with_base(base: &Path, path: &PathBuf) -> PackageResult<Self> {
        // First join the paths
        let joined = base.join(path);

        // Then canonicalize to get the absolute path
        let canonical = joined.canonicalize().map_err(|e| {
            crate::errors::PackageError::Generic(format!(
                "Failed to canonicalize path {}: {}",
                joined.display(),
                e
            ))
        })?;

        Ok(Self(canonical))
    }

    /// Return the path as a PathBuf
    pub fn path(&self) -> &PathBuf {
        &self.0
    }
}

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
    fn test_new_with_base() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();
        let base = PathBuf::from(temp_dir.path());
        let path = PathBuf::from("test.txt");
        let package_path = PackagePath::new_with_base(&base, &path).unwrap();
        assert_eq!(
            package_path.path(),
            &base.join(path).canonicalize().unwrap()
        );

        let temp_dir = tempfile::tempdir().unwrap();

        // Create the directory structure: temp_dir/A/B/C
        let a = temp_dir.path().join("A");
        fs::create_dir_all(&a).unwrap();
        let b = a.join("B");
        fs::create_dir_all(&b).unwrap();
        let c = b.join("C");
        fs::create_dir_all(&c).unwrap();

        // Test going from C to B using relative path
        let relative_path = PathBuf::from("../../B");
        let package_path = PackagePath::new_with_base(&c, &relative_path).unwrap();

        // The result should be the canonicalized path to B
        assert_eq!(package_path.path(), &b.canonicalize().unwrap());
    }
}
