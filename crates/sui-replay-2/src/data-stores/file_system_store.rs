// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! File system implementation of the replay interfaces: `TransactionStore`, `EpochStore`,
//! and `ObjectStore`.
//! Data is persisted on disk under a simple, human-inspectable directory layout.
//!
//! # Directory Structure
//!
//! ```text
//! ~/.replay_data_store/
//!   mainnet/ | testnet/ | custom/
//!     transaction/
//!       <tx_digest>              (BCS: TransactionFileData)
//!     epoch/
//!       <epoch_id>               (BCS: EpochFileData)
//!     objects/
//!       <object_id>/
//!         <version>              (BCS: sui_types::object::Object)
//!         root_versions          (CSV lines: "<max_version>,<actual_version>")
//!         checkpoint_versions    (CSV lines: "<checkpoint>,<actual_version>")
//! ```
//!
//! - The top-level directory is fixed to `~/.replay_data_store`.
//! - The next level is determined by the configured node: `mainnet`, `testnet`, or `custom` for
//!   any `Node::Custom(_)` value.
//!
//! # File Formats
//!
//! - Transaction files in `transaction/<tx_digest>` store `TransactionFileData` as BCS, which
//!   includes the original `TransactionData`, its `TransactionEffects`, and the execution
//!   `checkpoint`.
//! - Epoch files in `epoch/<epoch_id>` store `EpochFileData` as BCS, capturing epoch metadata.
//! - Object files in `objects/<object_id>/<version>` store the `Object` at the corresponding
//!   version as BCS.
//!
//! # Version Mapping Files
//!
//! The `root_versions` and `checkpoint_versions` files provide stable lookups for queries
//! that are not targeted to a specific version. Each line is a comma-separated pair and the
//! files can be edited manually or updated by this store:
//!
//! - `root_versions`: maps a root version bound to the actual stored version.
//!   - line format: `<max_version>,<actual_version>`
//!   - example: `4,3` means the highest version not exceeding 4 is 3
//!
//! - `checkpoint_versions`: maps a checkpoint to the actual stored version for the object
//!   at that point.
//!   - line format: `<checkpoint>,<actual_version>`
//!   - example: `100,1` means at checkpoint 100 the object version is 1
//!
//! These mapping files allow answering `VersionQuery::RootVersion(_)` and
//! `VersionQuery::AtCheckpoint(_)` without requiring a full index; the store writes them as
//! it learns the concrete versions.

use crate::{
    replay_interface::{
        EpochData, EpochStore, EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter,
        StoreSummary, TransactionInfo, TransactionStore, TransactionStoreWriter, VersionQuery,
    },
    Node,
};
use anyhow::{anyhow, Context};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
};
use sui_types::{
    base_types::ObjectID,
    committee::ProtocolVersion,
    effects::TransactionEffects,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
    transaction::TransactionData,
};

/// Serializable wrapper for transaction data stored in files
#[derive(serde::Serialize, serde::Deserialize)]
struct TransactionFileData {
    pub data: TransactionData,
    pub effects: TransactionEffects,
    pub checkpoint: u64,
}

/// Serializable wrapper for epoch data stored in files
#[derive(serde::Serialize, serde::Deserialize)]
struct EpochFileData {
    pub epoch_id: u64,
    pub protocol_version: u64,
    pub rgp: u64,
    pub start_timestamp: u64,
}

impl From<EpochFileData> for EpochData {
    fn from(file_data: EpochFileData) -> Self {
        EpochData {
            epoch_id: file_data.epoch_id,
            protocol_version: file_data.protocol_version,
            rgp: file_data.rgp,
            start_timestamp: file_data.start_timestamp,
        }
    }
}

impl From<EpochData> for EpochFileData {
    fn from(epoch_data: EpochData) -> Self {
        EpochFileData {
            epoch_id: epoch_data.epoch_id,
            protocol_version: epoch_data.protocol_version,
            rgp: epoch_data.rgp,
            start_timestamp: epoch_data.start_timestamp,
        }
    }
}

/// File system implementation of the replay interfaces
pub struct FileSystemStore {
    node: Node,
    base_path: PathBuf,
    metrics: FsStoreMetrics,
    // In-memory caches of version mappings per object id
    root_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
    checkpoint_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
}

impl FileSystemStore {
    pub fn new(node: Node) -> Result<Self, anyhow::Error> {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow!("Unable to determine home directory"))?;
        let base_path = PathBuf::from(home_dir).join(".replay_data_store");
        Ok(Self {
            node,
            base_path,
            metrics: FsStoreMetrics::default(),
            root_versions_map: RwLock::new(BTreeMap::new()),
            checkpoint_versions_map: RwLock::new(BTreeMap::new()),
        })
    }

    fn node_dir(&self) -> PathBuf {
        self.base_path.join(self.node.node_dir())
    }

    fn transaction_dir(&self) -> PathBuf {
        self.node_dir().join("transaction")
    }

    fn epoch_dir(&self) -> PathBuf {
        self.node_dir().join("epoch")
    }

    fn objects_dir(&self) -> PathBuf {
        self.node_dir().join("objects")
    }

    fn root_versions_path(&self, object_id: &ObjectID) -> PathBuf {
        self.objects_dir()
            .join(object_id.to_string())
            .join("root_versions")
    }

    fn checkpoint_versions_path(&self, object_id: &ObjectID) -> PathBuf {
        self.objects_dir()
            .join(object_id.to_string())
            .join("checkpoint_versions")
    }

    fn read_bcs_file<T: serde::de::DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<T, anyhow::Error> {
        let bytes =
            fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("Failed to deserialize BCS data from: {}", path.display()))
    }

    fn write_bcs_file<T: serde::Serialize>(
        &self,
        path: &Path,
        data: &T,
    ) -> Result<(), anyhow::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        let bytes = bcs::to_bytes(data)
            .with_context(|| format!("Failed to serialize BCS data for: {}", path.display()))?;
        fs::write(path, bytes)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        Ok(())
    }

    fn read_version_mapping(&self, path: &Path) -> Result<BTreeMap<u64, u64>, anyhow::Error> {
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read version mapping file: {}", path.display()))?;
        let mut mapping = BTreeMap::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() != 2 {
                return Err(anyhow!(
                    "Invalid format in version mapping file {}: expected 'key,version', got '{}'",
                    path.display(),
                    line
                ));
            }
            let key: u64 = parts[0].trim().parse().with_context(|| {
                format!("Failed to parse key '{}' in {}", parts[0], path.display())
            })?;
            let version: u64 = parts[1].trim().parse().with_context(|| {
                format!(
                    "Failed to parse version '{}' in {}",
                    parts[1],
                    path.display()
                )
            })?;
            mapping.insert(key, version);
        }
        Ok(mapping)
    }

    fn write_full_version_mapping(
        &self,
        path: &Path,
        mapping: &BTreeMap<u64, u64>,
    ) -> Result<(), anyhow::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        let mut content = String::new();
        for (k, v) in mapping {
            content.push_str(&format!("{},{}\n", k, v));
        }
        fs::write(path, content)
            .with_context(|| format!("Failed to write version mapping file: {}", path.display()))?;
        Ok(())
    }

    fn load_root_mapping(&self, object_id: &ObjectID) -> Result<(), anyhow::Error> {
        if self
            .root_versions_map
            .read()
            .unwrap()
            .contains_key(object_id)
        {
            return Ok(());
        }
        let path = self.root_versions_path(object_id);
        let mapping = self.read_version_mapping(&path)?;
        self.root_versions_map
            .write()
            .unwrap()
            .insert(*object_id, mapping);
        Ok(())
    }

    fn load_checkpoint_mapping(&self, object_id: &ObjectID) -> Result<(), anyhow::Error> {
        if self
            .checkpoint_versions_map
            .read()
            .unwrap()
            .contains_key(object_id)
        {
            return Ok(());
        }
        let path = self.checkpoint_versions_path(object_id);
        let mapping = self.read_version_mapping(&path)?;
        self.checkpoint_versions_map
            .write()
            .unwrap()
            .insert(*object_id, mapping);
        Ok(())
    }

    fn update_root_mapping(
        &self,
        object_id: &ObjectID,
        key: u64,
        version: u64,
    ) -> Result<(), anyhow::Error> {
        self.load_root_mapping(object_id)?;
        {
            let mut maps = self.root_versions_map.write().unwrap();
            let entry = maps.get_mut(object_id).unwrap();
            entry.insert(key, version);
            let path = self.root_versions_path(object_id);
            self.write_full_version_mapping(&path, entry)?;
        }
        Ok(())
    }

    fn update_checkpoint_mapping(
        &self,
        object_id: &ObjectID,
        key: u64,
        version: u64,
    ) -> Result<(), anyhow::Error> {
        self.load_checkpoint_mapping(object_id)?;
        {
            let mut maps = self.checkpoint_versions_map.write().unwrap();
            let entry = maps.get_mut(object_id).unwrap();
            entry.insert(key, version);
            let path = self.checkpoint_versions_path(object_id);
            self.write_full_version_mapping(&path, entry)?;
        }
        Ok(())
    }

    fn get_object_by_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> Result<Option<(Object, u64)>, anyhow::Error> {
        let object_dir = self.objects_dir().join(object_id.to_string());
        let version_file = object_dir.join(version.to_string());
        if !version_file.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&version_file)
            .map(|obj| Some((obj, version)))
    }

    fn get_object_by_root_version(
        &self,
        object_id: &ObjectID,
        max_version: u64,
    ) -> Result<Option<(Object, u64)>, anyhow::Error> {
        self.load_root_mapping(object_id)?;
        let maps = self.root_versions_map.read().unwrap();
        if let Some(map) = maps.get(object_id) {
            if let Some(&actual_version) = map.get(&max_version) {
                return self.get_object_by_version(object_id, actual_version);
            }
        }
        Ok(None)
    }

    fn get_object_by_checkpoint(
        &self,
        object_id: &ObjectID,
        checkpoint: u64,
    ) -> Result<Option<(Object, u64)>, anyhow::Error> {
        self.load_checkpoint_mapping(object_id)?;
        let maps = self.checkpoint_versions_map.read().unwrap();
        if let Some(map) = maps.get(object_id) {
            if let Some(&actual_version) = map.get(&checkpoint) {
                return self.get_object_by_version(object_id, actual_version);
            }
        }
        Ok(None)
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }
    pub fn node(&self) -> &Node {
        &self.node
    }
}

impl TransactionStore for FileSystemStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, anyhow::Error> {
        let file_path = self.transaction_dir().join(tx_digest);
        if !file_path.exists() {
            self.metrics.txn_miss.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }
        let txn_data: TransactionFileData = match self
            .read_bcs_file(&file_path)
            .with_context(|| format!("Failed to load transaction data for digest: {}", tx_digest))
        {
            Ok(v) => v,
            Err(e) => {
                self.metrics.txn_error.fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.metrics.txn_hit.fetch_add(1, Ordering::Relaxed);
        Ok(Some(TransactionInfo {
            data: txn_data.data,
            effects: txn_data.effects,
            checkpoint: txn_data.checkpoint,
        }))
    }
}

impl EpochStore for FileSystemStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, anyhow::Error> {
        let file_path = self.epoch_dir().join(epoch.to_string());
        if !file_path.exists() {
            self.metrics.epoch_miss.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }
        let epoch_file_data: EpochFileData = match self
            .read_bcs_file(&file_path)
            .with_context(|| format!("Failed to load epoch data for epoch: {}", epoch))
        {
            Ok(v) => v,
            Err(e) => {
                self.metrics.epoch_error.fetch_add(1, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
        Ok(Some(epoch_file_data.into()))
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, anyhow::Error> {
        match self.epoch_info(epoch) {
            Ok(Some(epoch_data)) => {
                self.metrics.proto_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(ProtocolConfig::get_for_version(
                    ProtocolVersion::new(epoch_data.protocol_version),
                    self.chain(),
                )))
            }
            Ok(None) => {
                self.metrics.proto_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.metrics.proto_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

impl ObjectStore for FileSystemStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            let (object_and_version_res, hit_ctr, miss_ctr, err_ctr) = match &key.version_query {
                VersionQuery::Version(version) => (
                    self.get_object_by_version(&key.object_id, *version),
                    &self.metrics.obj_version_hit,
                    &self.metrics.obj_version_miss,
                    &self.metrics.obj_version_error,
                ),
                VersionQuery::RootVersion(max_version) => (
                    self.get_object_by_root_version(&key.object_id, *max_version),
                    &self.metrics.obj_root_hit,
                    &self.metrics.obj_root_miss,
                    &self.metrics.obj_root_error,
                ),
                VersionQuery::AtCheckpoint(checkpoint) => (
                    self.get_object_by_checkpoint(&key.object_id, *checkpoint),
                    &self.metrics.obj_checkpoint_hit,
                    &self.metrics.obj_checkpoint_miss,
                    &self.metrics.obj_checkpoint_error,
                ),
            };

            match object_and_version_res {
                Ok(object_and_version) => {
                    if object_and_version.is_some() {
                        hit_ctr.fetch_add(1, Ordering::Relaxed);
                    } else {
                        miss_ctr.fetch_add(1, Ordering::Relaxed);
                    }
                    results.push(object_and_version);
                }
                Err(e) => {
                    err_ctr.fetch_add(1, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
        Ok(results)
    }
}

impl TransactionStoreWriter for FileSystemStore {
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), anyhow::Error> {
        let file_path = self.transaction_dir().join(tx_digest);
        let txn_file_data = TransactionFileData {
            data: transaction_info.data,
            effects: transaction_info.effects,
            checkpoint: transaction_info.checkpoint,
        };
        self.write_bcs_file(&file_path, &txn_file_data)
    }
}

impl EpochStoreWriter for FileSystemStore {
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), anyhow::Error> {
        let file_path = self.epoch_dir().join(epoch.to_string());
        let epoch_file_data = EpochFileData::from(epoch_data);
        self.write_bcs_file(&file_path, &epoch_file_data)
    }
}

impl ObjectStoreWriter for FileSystemStore {
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), anyhow::Error> {
        let object_dir = self.objects_dir().join(key.object_id.to_string());
        let object_file = object_dir.join(actual_version.to_string());
        self.write_bcs_file(&object_file, &object)?;
        match &key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(max_version) => {
                self.update_root_mapping(&key.object_id, *max_version, actual_version)?;
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                self.update_checkpoint_mapping(&key.object_id, *checkpoint, actual_version)?;
            }
        }
        Ok(())
    }
}

impl StoreSummary for FileSystemStore {
    fn summary<W: std::io::Write>(&self, w: &mut W) -> anyhow::Result<()> {
        let m = &self.metrics;
        let txn_hit = m.txn_hit.load(Ordering::Relaxed);
        let txn_miss = m.txn_miss.load(Ordering::Relaxed);
        let txn_err = m.txn_error.load(Ordering::Relaxed);
        let epoch_hit = m.epoch_hit.load(Ordering::Relaxed);
        let epoch_miss = m.epoch_miss.load(Ordering::Relaxed);
        let epoch_err = m.epoch_error.load(Ordering::Relaxed);
        let proto_hit = m.proto_hit.load(Ordering::Relaxed);
        let proto_miss = m.proto_miss.load(Ordering::Relaxed);
        let proto_err = m.proto_error.load(Ordering::Relaxed);
        let obj_v_hit = m.obj_version_hit.load(Ordering::Relaxed);
        let obj_v_miss = m.obj_version_miss.load(Ordering::Relaxed);
        let obj_v_err = m.obj_version_error.load(Ordering::Relaxed);
        let obj_r_hit = m.obj_root_hit.load(Ordering::Relaxed);
        let obj_r_miss = m.obj_root_miss.load(Ordering::Relaxed);
        let obj_r_err = m.obj_root_error.load(Ordering::Relaxed);
        let obj_c_hit = m.obj_checkpoint_hit.load(Ordering::Relaxed);
        let obj_c_miss = m.obj_checkpoint_miss.load(Ordering::Relaxed);
        let obj_c_err = m.obj_checkpoint_error.load(Ordering::Relaxed);

        let total_hit = txn_hit + epoch_hit + proto_hit + obj_v_hit + obj_r_hit + obj_c_hit;
        let total_miss = txn_miss + epoch_miss + proto_miss + obj_v_miss + obj_r_miss + obj_c_miss;
        let total_err = txn_err + epoch_err + proto_err + obj_v_err + obj_r_err + obj_c_err;
        let obj_total_hit = obj_v_hit + obj_r_hit + obj_c_hit;
        let obj_total_miss = obj_v_miss + obj_r_miss + obj_c_miss;
        let obj_total_err = obj_v_err + obj_r_err + obj_c_err;

        writeln!(w, "FileSystemStore summary")?;
        writeln!(
            w,
            "  Overall: hit={} miss={} error={}",
            total_hit, total_miss, total_err
        )?;
        writeln!(
            w,
            "  Transaction: hit={} miss={} error={}",
            txn_hit, txn_miss, txn_err
        )?;
        writeln!(
            w,
            "  Epoch:       hit={} miss={} error={}",
            epoch_hit, epoch_miss, epoch_err
        )?;
        writeln!(
            w,
            "  Protocol:    hit={} miss={} error={}",
            proto_hit, proto_miss, proto_err
        )?;
        writeln!(
            w,
            "  Objects (all): hit={} miss={} error={}",
            obj_total_hit, obj_total_miss, obj_total_err
        )?;
        writeln!(w, "  Objects:")?;
        writeln!(
            w,
            "    Version:     hit={} miss={} error={}",
            obj_v_hit, obj_v_miss, obj_v_err
        )?;
        writeln!(
            w,
            "    RootVersion: hit={} miss={} error={}",
            obj_r_hit, obj_r_miss, obj_r_err
        )?;
        writeln!(
            w,
            "    Checkpoint:  hit={} miss={} error={}",
            obj_c_hit, obj_c_miss, obj_c_err
        )?;
        Ok(())
    }
}

#[derive(Default)]
struct FsStoreMetrics {
    // transactions
    txn_hit: AtomicU64,
    txn_miss: AtomicU64,
    txn_error: AtomicU64,
    // epochs
    epoch_hit: AtomicU64,
    epoch_miss: AtomicU64,
    epoch_error: AtomicU64,
    // protocol config
    proto_hit: AtomicU64,
    proto_miss: AtomicU64,
    proto_error: AtomicU64,
    // objects by query kind
    obj_version_hit: AtomicU64,
    obj_version_miss: AtomicU64,
    obj_version_error: AtomicU64,
    obj_root_hit: AtomicU64,
    obj_root_miss: AtomicU64,
    obj_root_error: AtomicU64,
    obj_checkpoint_hit: AtomicU64,
    obj_checkpoint_miss: AtomicU64,
    obj_checkpoint_error: AtomicU64,
}
