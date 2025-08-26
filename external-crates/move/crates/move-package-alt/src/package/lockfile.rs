// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use tracing::debug;

use crate::{
    errors::{FileHandle, PackageResult},
    schema::{PackageID, ParsedLockfile, Pin},
};

use super::{EnvironmentName, paths::PackagePath};

#[derive(Debug)]
pub struct Lockfiles {
    main: ParsedLockfile,
    file: FileHandle,
}

// TODO: instead of a Lockfile module, we should maybe have a publication information module I think; the
// in-memory pinned section is just the dependency graph

impl Lockfiles {
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
        let main: ParsedLockfile = toml_edit::de::from_str(file_id.source())?;

        Ok(Some(Lockfiles {
            main,
            file: file_id,
        }))
    }

    pub fn pins_for_env(&self, env: &EnvironmentName) -> Option<&BTreeMap<PackageID, Pin>> {
        self.main.pinned.get(env)
    }

    pub fn file(&self) -> FileHandle {
        self.file
    }
}
