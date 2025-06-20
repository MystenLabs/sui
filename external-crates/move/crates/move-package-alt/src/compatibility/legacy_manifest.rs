// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, bail};

use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::read_to_string,
    path::{Component, Path, PathBuf},
};

use crate::{
    compatibility::find_module_name_for_package,
    dependency::{
        UnpinnedDependencyInfo,
        external::ExternalDependency,
        git::UnpinnedGitDependency,
        local::LocalDependency,
        onchain::{ConstTrue, OnChainDependency},
    },
    errors::{Located, TheFile},
    flavor::MoveFlavor,
    package::{
        PackageName,
        layout::SourcePackageLayout,
        manifest::{Manifest, ManifestDependency},
    },
};

pub type LegacyAddressDeclarations = BTreeMap<String, Option<AccountAddress>>;
pub type LegacyDevAddressDeclarations = BTreeMap<String, AccountAddress>;
pub type LegacyVersion = (u64, u64, u64);

pub type LegacySubstitution = BTreeMap<String, LegacySubstOrRename>;

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct LegacyBuildInfo {
    pub language_version: Option<LegacyVersion>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LegacySubstOrRename {
    RenameFrom(String),
    Assign(AccountAddress),
}

/// Normalize the representation of `path` by eliminating redundant `.` components and applying `..`
/// component.  Does not access the filesystem (e.g. to resolve symlinks or test for file
/// existence), unlike `std::fs::canonicalize`.
///
/// Fails if the normalized path attempts to access the parent of a root directory or volume prefix,
/// or is prefixed by accesses to parent directories when `allow_cwd_parent` is false.
///
/// Returns the normalized path on success.
pub fn normalize_path(path: impl AsRef<Path>, allow_cwd_parent: bool) -> Result<PathBuf> {
    use Component::*;

    let mut stack = Vec::new();
    for component in path.as_ref().components() {
        match component {
            // Components that contribute to the path as-is.
            verbatim @ (Prefix(_) | RootDir | Normal(_)) => stack.push(verbatim),

            // Equivalent of a `.` path component -- can be ignored.
            CurDir => { /* nop */ }

            // Going up in the directory hierarchy, which may fail if that's not possible.
            ParentDir => match stack.last() {
                None | Some(ParentDir) => {
                    stack.push(ParentDir);
                }

                Some(Normal(_)) => {
                    stack.pop();
                }

                Some(CurDir) => {
                    unreachable!("Component::CurDir never added to the stack");
                }

                Some(RootDir | Prefix(_)) => bail!(
                    "Invalid path accessing parent of root directory: {}",
                    path.as_ref().to_string_lossy(),
                ),
            },
        }
    }

    let normalized: PathBuf = stack.iter().collect();
    if !allow_cwd_parent && stack.first() == Some(&ParentDir) {
        bail!(
            "Path cannot access parent of current directory: {}",
            normalized.to_string_lossy()
        );
    }

    Ok(normalized)
}
