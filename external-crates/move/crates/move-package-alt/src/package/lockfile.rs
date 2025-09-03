// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use tracing::debug;

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    schema::{PackageID, ParsedLockfile, Pin, Publication},
};

use super::{EnvironmentName, paths::PackagePath};

#[derive(Debug)]
pub struct Lockfiles<F: MoveFlavor> {
    main: ParsedLockfile<F>,
    file: FileHandle,
    ephemeral: BTreeMap<EnvironmentName, Publication<F>>,
    // TODO: probably should have separate file handles for ephemerals?
}

// TODO: instead of a Lockfile module, we should maybe have a publication information module I think; the
// in-memory pinned section is just the dependency graph

impl<F: MoveFlavor> Lockfiles<F> {
    /// Read `Move.lock` from `path`; returning [None] if it doesn't exist
    pub fn read_from_dir(path: &PackagePath) -> PackageResult<Option<Self>> {
        // Parse `Move.lock`
        debug!("reading lockfiles from {:?}", path);
        let lockfile_name = path.lockfile_path();
        if !lockfile_name.exists() {
            debug!("no lockfile found");
            return Ok(None);
        };

        let file_id = FileHandle::new(lockfile_name)?;
        let main: ParsedLockfile<F> = toml_edit::de::from_str(file_id.source())?;

        // Parse all `.Move.<env>.lock`
        debug!("reading ephemeral lockfiles");
        let mut ephemeral = BTreeMap::new();
        for (env, path) in path.env_lockfiles()? {
            let file_id = FileHandle::new(path)?;
            let metadata = toml_edit::de::from_str(file_id.source())?;

            ephemeral.insert(env.clone(), metadata);
        }

        Ok(Some(Lockfiles {
            main,
            ephemeral,
            file: file_id,
        }))
    }

    /// Serialize [self] into `Move.lock` and `Move.<env>.lock`.
    pub fn write_to(&self, path: &PackagePath) -> PackageResult<()> {
        std::fs::write(path.lockfile_path(), self.main.render_as_toml())?;

        for (env, entry) in &self.ephemeral {
            std::fs::write(path.lockfile_by_env_path(env), entry.render_as_toml())?;
        }

        Ok(())
    }

    pub fn pins_for_env(&self, env: &EnvironmentName) -> Option<&BTreeMap<PackageID, Pin>> {
        self.main.pinned.get(env)
    }

    pub fn file(&self) -> FileHandle {
        self.file
    }

    // TODO: handle ephemerals correctly
    /// Return the published metadata for all environments.
    pub fn published(&self) -> &BTreeMap<EnvironmentName, Publication<F>> {
        &self.main.published
    }

    // TODO: handle ephemerals correctly
    /// Return the published metadata for a specific environment.
    pub fn published_for_env(&self, env: &EnvironmentName) -> Option<Publication<F>> {
        self.main.published.get(env).cloned()
    }

    // TODO: ignores ephemerals and should probably be removed
    pub fn render_main_lockfile(&self) -> String {
        self.main.render_as_toml()
    }
}

#[cfg(test)]
mod tests {
    // TODO
}
