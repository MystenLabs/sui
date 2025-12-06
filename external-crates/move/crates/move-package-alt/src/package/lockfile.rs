// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    schema::{PackageID, ParsedLockfile, Pin},
};

use super::{EnvironmentName, package_lock::PackageSystemLock, paths::PackagePath};

#[derive(Debug)]
pub struct Lockfiles {
    main: ParsedLockfile,
    file: FileHandle,
}

// TODO: instead of a Lockfile module, we should maybe have a publication information module I think; the
// in-memory pinned section is just the dependency graph

impl Lockfiles {
    /// Read `Move.lock` from `path`; returning [None] if it doesn't exist
    pub fn read_from_dir<F: MoveFlavor>(
        path: &PackagePath,
        mtx: &PackageSystemLock,
    ) -> PackageResult<Option<Self>> {
        Ok(path
            .read_lockfile(mtx)?
            .map(|(file, main)| Lockfiles { main, file }))
    }

    pub fn pins_for_env(&self, env: &EnvironmentName) -> Option<&BTreeMap<PackageID, Pin>> {
        self.main.pinned.get(env)
    }

    pub fn file(&self) -> FileHandle {
        self.file
    }
}
