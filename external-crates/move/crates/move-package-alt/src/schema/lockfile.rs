use std::{collections::BTreeMap, path::PathBuf};

use crate::git::sha::GitSha;

use super::{Address, EnvironmentName, PackageName};

/// An identifier for a node in the package graph, used to index into the
/// `[pinned.<environment>]` table
type PackageID = String;

/// The serialized lockfile format
#[derive(Debug, Serialize, Deserialize)]
#[derive_where(Clone, Default)]
#[serde(bound = "")]
pub struct Lockfile {
    pinned: BTreeMap<EnvironmentName, BTreeMap<PackageName, Pin>>,
    #[serde(default)]
    published: BTreeMap<EnvironmentName, Publication>,
}

/// A serialized entry in the `[published.<environment>]` table of the lockfile
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Publication {
    published_at: Address,
    original_id: Address,
}

/// A serialized entry in the `[pinned.<environment>.<package-id>]` table of the lockfile
#[derive(Debug, Serialize, Deserialize)]
#[derive_where(Clone)]
pub struct Pin {
    /// Metadata about the package's source
    pub source: PinnedDependency,
    /// Contains the package's manifest digest. This is used to verify if a manifest has changed
    /// and re-pinning is required.
    pub manifest_digest: String,
    /// The package's dependencies, a map from the package name to the package id.
    pub deps: BTreeMap<PackageName, PackageID>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PinnedDependency {
    Local(LocalDependency),
    OnChain(OnChainDependency),
    Git(GitDependency),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalDependency {
    /// The path on the filesystem, relative to the location of the containing file
    local: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OnChainDependency {
    /// The published address of the dependency
    on_chain: Address,
}

/// A pinned dependency of the form `{git = "...", rev = "...", path = "..."}`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GitDependency {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    pub repo: String,

    /// The git commit or branch for the dependency.
    pub rev: GitSha,

    /// The path within the repository
    pub path: PathBuf,
}
