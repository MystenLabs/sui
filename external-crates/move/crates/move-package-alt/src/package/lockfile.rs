// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;
use toml_edit::{
    DocumentMut, InlineTable, Item, KeyMut, Table, Value,
    visit_mut::{VisitMut, visit_table_like_kv_mut, visit_table_mut},
};

use crate::{
    dependency::{Dependency, DependencySet},
    errors::{FileHandle, Located, LockfileError, PackageError, PackageResult},
    schema::{self, EnvironmentID, EnvironmentName, PackageName, Publication},
};

#[derive(Debug, Default, Clone)]
pub struct Lockfile {
    inner: schema::Lockfile,
    ephemeral: BTreeMap<EnvironmentName, schema::Publication>,
}

impl Lockfile {
    /// Read `Move.lock` and all `Move.<env>.lock` files from the directory at `path`.
    /// Returns a new empty [Lockfile] if `path` doesn't contain a `Move.lock`.
    pub fn read_from_dir(path: impl AsRef<Path>) -> PackageResult<Self> {
        // Parse `Move.lock`
        let lockfile_name = path.as_ref().join("Move.lock");
        if !lockfile_name.exists() {
            return Ok(Self::default());
        };

        let file_id = FileHandle::new(lockfile_name)?;
        let result = toml_edit::de::from_str::<schema::Lockfile>(file_id.source());

        let Ok(mut lockfiles) = result else {
            return Err(result.unwrap_err().into());
        };

        // Add in `Move.<env>.lock` files
        let mut ephemeral: BTreeMap<EnvironmentName, schema::Publication> = BTreeMap::new();
        let dir = std::fs::read_dir(path)?;
        for entry in dir {
            let Ok(file) = entry else { continue };

            let Some(env_name) = lockname_to_env_name(file.file_name()) else {
                continue;
            };

            let file_id = FileHandle::new(file.path())?;

            let metadata = toml_edit::de::from_str::<schema::Publication>(file_id.source())?;

            ephemeral.insert(env_name.clone(), metadata);
        }

        Ok(Self {
            inner: lockfiles,
            ephemeral,
        })
    }

    /// Serialize [self] into `Move.lock` and `Move.<env>.lock`.
    ///
    /// The [PublishedMetadata] in `self.published.<env>` are partitioned: if `env` is in [envs]
    /// then it is saved to `Move.lock` (and `Move.<env>.lock` is deleted), otherwise the metadata
    /// is stored in `Move.<env>.lock`.
    pub fn write_to(
        &self,
        path: impl AsRef<Path>,
        envs: BTreeMap<EnvironmentName, EnvironmentID>,
    ) -> PackageResult<()> {
        let mut output: schema::Lockfile = self.inner.clone();
        let (pubs, locals): (BTreeMap<_, _>, BTreeMap<_, _>) = output
            .published
            .into_iter()
            .partition(|(env_name, metadata)| envs.contains_key(env_name));
        output.published = pubs;

        std::fs::write(path.as_ref().join("Move.lock"), output.render_as_toml())?;

        for (chain, metadata) in locals {
            std::fs::write(
                path.as_ref().join(format!("Move.{}.lock", chain)),
                metadata.render_as_toml(),
            )?;
        }

        for chain in output.published.keys() {
            let _ = std::fs::remove_file(path.as_ref().join(format!("Move.{}.lock", chain)));
        }

        Ok(())
    }

    pub fn render_as_toml(&self) -> String {
        self.inner.render_as_toml()
    }

    /// Return the published metadata for all environments.
    fn published(&self) -> &BTreeMap<EnvironmentName, Publication> {
        &self.inner.published
    }

    /// Return the published metadata for a specific environment.
    pub fn published_for_env(&self, env: &EnvironmentName) -> Option<Publication> {
        self.inner.published.get(env).cloned()
    }

    /// Return the pinned dependencies for the given environment, if it exists in the lockfile.
    pub fn pinned_deps_for_env(
        &self,
        env: &EnvironmentName,
    ) -> Option<&BTreeMap<PackageName, schema::Pin>> {
        self.inner.pinned.get(env)
    }

    /// Return a map that has an environment as key and the dependencies for that environment.
    pub fn pinned_deps(&self) -> &DependencySet<schema::Pin> {
        &self.inner.pinned
    }
}

/// Given a filename of the form `Move.<env>.lock`, returns `<env>`.
fn lockname_to_env_name(filename: OsString) -> Option<String> {
    let Ok(filename) = filename.into_string() else {
        return None;
    };

    let prefix = "Move.";
    let suffix = ".lock";

    if filename.starts_with(prefix) && filename.ends_with(suffix) {
        let start_index = prefix.len();
        let end_index = filename.len() - suffix.len();

        if start_index < end_index {
            return Some(filename[start_index..end_index].to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockname_to_env_name() {
        assert_eq!(
            lockname_to_env_name(OsString::from("Move.test.lock")),
            Some("test".to_string())
        );
        assert_eq!(
            lockname_to_env_name(OsString::from("Move.3vcga23.lock")),
            Some("3vcga23".to_string())
        );
        assert_eq!(
            lockname_to_env_name(OsString::from("Mve.test.lock.lock")),
            None
        );

        assert_eq!(lockname_to_env_name(OsString::from("Move.lock")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.test.loc")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.testloc")), None);
        assert_eq!(lockname_to_env_name(OsString::from("Move.test")), None);
    }
}
