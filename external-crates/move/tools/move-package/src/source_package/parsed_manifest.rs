// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};

use crate::Architecture;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::symbol::Symbol;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub type NamedAddress = Symbol;
pub type PackageName = Symbol;
pub type FileName = Symbol;
pub type PackageDigest = Symbol;
pub type DepOverride = bool;

pub type AddressDeclarations = BTreeMap<NamedAddress, Option<AccountAddress>>;
pub type DevAddressDeclarations = BTreeMap<NamedAddress, AccountAddress>;
pub type Version = (u64, u64, u64);
pub type Dependencies = BTreeMap<PackageName, Dependency>;
pub type Substitution = BTreeMap<NamedAddress, SubstOrRename>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SourceManifest {
    pub package: PackageInfo,
    pub addresses: Option<AddressDeclarations>,
    pub dev_address_assignments: Option<DevAddressDeclarations>,
    pub build: Option<BuildInfo>,
    pub dependencies: Dependencies,
    pub dev_dependencies: Dependencies,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PackageInfo {
    pub name: PackageName,
    pub version: Version,
    pub authors: Vec<Symbol>,
    pub license: Option<Symbol>,
    pub custom_properties: BTreeMap<Symbol, String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Dependency {
    /// Parametrised by the binary that will resolve packages for this dependency.
    External(Symbol),
    Internal(InternalDependency),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InternalDependency {
    pub kind: DependencyKind,
    pub subst: Option<Substitution>,
    pub version: Option<Version>,
    pub digest: Option<PackageDigest>,
    pub dep_override: DepOverride,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DependencyKind {
    Local(PathBuf),
    Git(GitInfo),
    Custom(CustomDepInfo),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GitInfo {
    /// The git clone url to download from
    pub git_url: Symbol,
    /// The git revision, AKA, a commit SHA
    pub git_rev: Symbol,
    /// The path under this repo where the move package can be found -- e.g.,
    /// 'language/move-stdlib`
    pub subdir: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CustomDepInfo {
    /// The url of the node to download from
    pub node_url: Symbol,
    /// The address where the package is published. The representation depends
    /// on the registered node resolver.
    pub package_address: Symbol,
    /// The package's name (i.e. the dependency name).
    pub package_name: Symbol,
    /// The path under this repo where the move package can be found
    pub subdir: PathBuf,
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct BuildInfo {
    pub language_version: Option<Version>,
    pub architecture: Option<Architecture>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum SubstOrRename {
    RenameFrom(NamedAddress),
    Assign(AccountAddress),
}

impl DependencyKind {
    /// Given a dependency `self` assumed to be defined relative to a `parent` dependency which can
    /// itself be defined in terms of some grandparent dependency (not provided), update `self` to
    /// be defined relative to its grandparent.
    ///
    /// Fails if the resulting dependency cannot be described relative to the grandparent, because
    /// its path is not valid (does not point to a valid location in the filesystem for local
    /// dependencies, or within the repository for remote dependencies).
    pub fn reroot(&mut self, parent: &DependencyKind) -> Result<()> {
        let mut parent = parent.clone();

        match (&mut parent, &self) {
            // If `self` is a git or custom dependency kind, it does not need to be re-rooted
            // because its URI is already absolute. (i.e. the location of an absolute URI does not
            // change if referenced relative to some other URI).
            (_, DependencyKind::Git(_) | DependencyKind::Custom(_)) => return Ok(()),

            (DependencyKind::Local(parent), DependencyKind::Local(subdir)) => {
                parent.push(subdir);
                *parent = normalize_path(&parent, /* allow_cwd_parent */ true)?;
            }

            (DependencyKind::Git(git), DependencyKind::Local(subdir)) => {
                git.subdir.push(subdir);
                git.subdir = normalize_path(&git.subdir, /* allow_cwd_parent */ false)?;
            }

            (DependencyKind::Custom(custom), DependencyKind::Local(subdir)) => {
                custom.subdir.push(subdir);
                custom.subdir = normalize_path(&custom.subdir, /* allow_cwd_parent */ false)?;
            }
        };

        *self = parent;
        Ok(())
    }
}

/// Default `DependencyKind` is the one that acts as the left and right identity to
/// `DependencyKind::rerooted` (modulo path normalization).
impl Default for DependencyKind {
    fn default() -> Self {
        DependencyKind::Local(PathBuf::new())
    }
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
