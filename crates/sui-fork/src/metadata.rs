// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fork-local metadata sidecar.
//!
//! Raw chain data lives in `sui-rpc-store`. This module only owns data-dir
//! layout, the immutable seed manifest, and completion markers for the
//! GraphQL inventory scans that are intentionally initialized from remote data.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::bail;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::Node;
use crate::seed::SeedManifest;

/// Environment variable name for an explicit fork data directory.
const SUI_FORK_DATA_ENV: &str = "SUI_FORK_DATA";
/// Directory name appended to XDG_DATA_HOME or $HOME on Unix, or %APPDATA% on Windows, when an
/// explicit data directory is not provided.
const DATA_DIR: &str = "sui_fork_data";
/// JSON file containing immutable fork metadata and optional pre-fork seed metadata.
const SEED_MANIFEST_FILE: &str = "seed_manifest.json";
/// JSON file tracking remote inventory scans that have been fully saved into `sui-rpc-store`.
const INVENTORY_METADATA_FILE: &str = "inventory_metadata.json";

/// Tracks which remote "inventory" scans have completed.
///
/// An *inventory* is the one-time, full GraphQL enumeration of every object owned
/// by an address / owned by an object / matching a type at the fork checkpoint.
/// It is distinct from the *index* it populates: running an inventory writes the
/// results into `sui-rpc-store`'s `object_by_owner` / `object_by_type` index CFs.
/// Each set below records the owners/types whose inventory has finished, so later
/// reads serve straight from the local index instead of re-scanning GraphQL. An
/// owner that legitimately owns nothing still counts as completed.
#[derive(Default, serde::Deserialize, serde::Serialize)]
struct InventoryMetadata {
    #[serde(default)]
    completed_address_owners: BTreeSet<SuiAddress>,
    #[serde(default)]
    completed_object_owners: BTreeSet<ObjectID>,
    #[serde(default)]
    completed_type_filters: BTreeSet<String>,
}

/// Fork-local metadata and data-dir layout.
#[derive(Clone)]
pub(crate) struct ForkMetadataStore {
    root: PathBuf,
}

impl ForkMetadataStore {
    /// Create a new fork metadata store. Explicit data directories are used as the exact
    /// root; otherwise the root is `{base_path}/{network_name}/forked_at_{checkpoint}`.
    pub(crate) fn new(
        node: &Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        data_dir: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let root = match data_dir {
            Some(dir) => dir,
            None => Self::default_root(node, forked_at_checkpoint)?,
        };
        Ok(Self { root })
    }

    /// Create a fork metadata store with an explicit root directory.
    pub(crate) fn new_with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Return the root directory for this fork's local state.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve the default base path for on-disk metadata.
    pub(crate) fn base_path() -> Result<PathBuf, Error> {
        Self::base_path_from_env(|key| env::var_os(key))
    }

    /// Resolve the default base path for on-disk metadata, using the provided `get_env` function
    /// to access environment variables.
    fn base_path_from_env(
        mut get_env: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<PathBuf, Error> {
        if let Some(dir) = get_env(SUI_FORK_DATA_ENV) {
            return Ok(PathBuf::from(dir));
        }

        Self::default_data_root_from_env(get_env)
    }

    #[cfg(unix)]
    fn default_data_root_from_env(
        mut get_env: impl FnMut(&str) -> Option<OsString>,
    ) -> anyhow::Result<PathBuf> {
        if let Some(data_dir) = get_env("XDG_DATA_HOME") {
            return Ok(PathBuf::from(data_dir).join(DATA_DIR));
        }

        let home = get_env("HOME").context("could not find $HOME directory")?;
        Ok(PathBuf::from(home).join(format!(".{}", DATA_DIR)))
    }

    #[cfg(windows)]
    fn default_data_root_from_env(
        mut get_env: impl FnMut(&str) -> Option<OsString>,
    ) -> anyhow::Result<PathBuf> {
        let app_data = get_env("APPDATA").context("could not find %APPDATA% directory")?;
        Ok(PathBuf::from(app_data).join(DATA_DIR))
    }

    /// Construct the default root directory for a given node and fork checkpoint.
    fn default_root(
        node: &Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Result<PathBuf, Error> {
        Ok(Self::root_from_base(
            Self::base_path()?,
            node,
            forked_at_checkpoint,
        ))
    }

    /// Construct the path from base path joined with `{network_name}/forked_at_{checkpoint}`.
    fn root_from_base(
        base: PathBuf,
        node: &Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> PathBuf {
        base.join(node.network_name())
            .join(format!("forked_at_{}", forked_at_checkpoint))
    }

    /// Return the path to the seed manifest for this fork directory.
    pub(crate) fn seed_manifest_path(&self) -> PathBuf {
        self.root.join(SEED_MANIFEST_FILE)
    }

    fn inventory_metadata_path(&self) -> PathBuf {
        self.root.join(INVENTORY_METADATA_FILE)
    }

    pub(crate) fn object_owner_inventory_complete(&self, owner: ObjectID) -> anyhow::Result<bool> {
        Ok(self
            .read_inventory_metadata()?
            .completed_object_owners
            .contains(&owner))
    }

    pub(crate) fn address_owner_inventory_complete(
        &self,
        owner: SuiAddress,
    ) -> anyhow::Result<bool> {
        Ok(self
            .read_inventory_metadata()?
            .completed_address_owners
            .contains(&owner))
    }

    pub(crate) fn mark_address_owner_inventory_complete(
        &self,
        owner: SuiAddress,
    ) -> anyhow::Result<()> {
        let mut metadata = self.read_inventory_metadata()?;
        metadata.completed_address_owners.insert(owner);
        self.write_inventory_metadata(&metadata)
    }

    pub(crate) fn mark_object_owner_inventory_complete(
        &self,
        owner: ObjectID,
    ) -> anyhow::Result<()> {
        let mut metadata = self.read_inventory_metadata()?;
        metadata.completed_object_owners.insert(owner);
        self.write_inventory_metadata(&metadata)
    }

    pub(crate) fn type_inventory_complete(&self, type_filter: &str) -> anyhow::Result<bool> {
        Ok(self
            .read_inventory_metadata()?
            .completed_type_filters
            .contains(type_filter))
    }

    pub(crate) fn mark_type_inventory_complete(&self, type_filter: &str) -> anyhow::Result<()> {
        let mut metadata = self.read_inventory_metadata()?;
        metadata
            .completed_type_filters
            .insert(type_filter.to_owned());
        self.write_inventory_metadata(&metadata)
    }

    /// Return whether the immutable seed manifest exists for this fork directory.
    pub(crate) fn seed_manifest_exists(&self) -> bool {
        self.seed_manifest_path().exists()
    }

    /// Read the immutable seed manifest from disk.
    pub(crate) fn read_seed_manifest(&self) -> anyhow::Result<SeedManifest> {
        read_json(&self.seed_manifest_path(), "seed manifest")
    }

    /// Write the immutable seed manifest, failing if one already exists.
    pub(crate) fn write_seed_manifest(&self, manifest: &SeedManifest) -> anyhow::Result<()> {
        let path = self.seed_manifest_path();
        if path.exists() {
            bail!("Seed manifest already exists: {}", path.display());
        }
        write_json_exclusive(&path, manifest, "seed manifest")
    }

    fn read_inventory_metadata(&self) -> anyhow::Result<InventoryMetadata> {
        let path = self.inventory_metadata_path();
        if !path.exists() {
            return Ok(InventoryMetadata::default());
        }
        read_json(&path, "inventory metadata")
    }

    fn write_inventory_metadata(&self, metadata: &InventoryMetadata) -> anyhow::Result<()> {
        write_json_replace(
            &self.inventory_metadata_path(),
            metadata,
            "inventory metadata",
        )
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, description: &str) -> anyhow::Result<T> {
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read {description}: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("Failed to deserialize {description}: {}", path.display()))
}

fn write_json_exclusive<T: serde::Serialize>(
    path: &Path,
    value: &T,
    description: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("Failed to serialize {description}: {}", path.display()))?;
    fs::write(&tmp_path, bytes)
        .with_context(|| format!("Failed to write {description}: {}", tmp_path.display()))?;
    if path.exists() {
        fs::remove_file(&tmp_path)
            .with_context(|| format!("Failed to remove temporary file: {}", tmp_path.display()))?;
        bail!("{} already exists: {}", description, path.display());
    }
    fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to replace {description}: {}", path.display()))
}

fn write_json_replace<T: serde::Serialize>(
    path: &Path,
    value: &T,
    description: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("Failed to serialize {description}: {}", path.display()))?;
    fs::write(&tmp_path, bytes)
        .with_context(|| format!("Failed to write {description}: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to replace {description}: {}", path.display()))
}

#[cfg(test)]
#[path = "tests/metadata.rs"]
mod tests;
