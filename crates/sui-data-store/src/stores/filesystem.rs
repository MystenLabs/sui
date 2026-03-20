// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! File-system backed store with live object persistence only.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Error, Result, anyhow};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::{Object, Owner},
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter, VersionQuery, node::Node,
};

pub const DATA_STORE_DIR: &str = ".sui_data_store";
pub const NODE_MAPPING_FILE: &str = "node_mapping.csv";
pub const OBJECTS_DIR: &str = "objects";
pub const TRANSACTION_DIR: &str = "transaction";
pub const EPOCH_DIR: &str = "epoch";
pub const CHECKPOINT_DIR: &str = "checkpoint";
pub const CHECKPOINT_FILE_EXTENSION: &str = "binpb.zst";
pub const CHECKPOINT_DIGEST_INDEX_FILE: &str = "checkpoint_digest_index.csv";
pub const CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE: &str = "contents_digest_index.csv";
pub const CHECKPOINT_LATEST_FILE: &str = "latest";
pub const ROOT_VERSIONS_FILE: &str = "root_versions";
pub const CHECKPOINT_VERSIONS_FILE: &str = "checkpoint_versions";

#[derive(Default, Debug)]
struct FileSystemMetrics {
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

/// Persistent file-system store.
#[derive(Debug)]
pub struct FileSystemStore {
    node: Node,
    base_path: PathBuf,
    metrics: FileSystemMetrics,
    root_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
    checkpoint_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
}

impl FileSystemStore {
    /// Create a store rooted at the default data-store path.
    pub fn new(node: Node) -> Result<Self, Error> {
        Ok(Self {
            node,
            base_path: Self::base_path()?,
            metrics: FileSystemMetrics::default(),
            root_versions_map: RwLock::new(BTreeMap::new()),
            checkpoint_versions_map: RwLock::new(BTreeMap::new()),
        })
    }

    /// Create a store rooted at an explicit path.
    pub fn new_with_path(node: Node, full_path: PathBuf) -> Result<Self, Error> {
        Ok(Self {
            node,
            base_path: full_path,
            metrics: FileSystemMetrics::default(),
            root_versions_map: RwLock::new(BTreeMap::new()),
            checkpoint_versions_map: RwLock::new(BTreeMap::new()),
        })
    }

    /// Resolve the default base path for on-disk storage.
    pub fn base_path() -> Result<PathBuf, Error> {
        let home_dir = std::env::var("SUI_DATA_STORE")
            .or_else(|_| std::env::var("SUI_CONFIG_DIR"))
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                anyhow!(
                    "Cannot determine home directory. Define a SUI_DATA_STORE environment variable"
                )
            })?;
        Ok(PathBuf::from(home_dir).join(DATA_STORE_DIR))
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Return the configured base path.
    pub fn store_path(&self) -> &PathBuf {
        &self.base_path
    }

    fn node_key(&self) -> &str {
        match &self.node {
            Node::Mainnet => "mainnet",
            Node::Testnet => "testnet",
            Node::Devnet => "devnet",
            Node::Custom(url) => url.as_str(),
        }
    }

    fn node_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.base_path.join(self.get_chain_id_for_node()?))
    }

    fn objects_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.node_dir()?.join(OBJECTS_DIR))
    }

    fn object_dir(&self, object_id: &ObjectID) -> Result<PathBuf, Error> {
        Ok(self.objects_dir()?.join(object_id.to_string()))
    }

    fn object_version_path(&self, object_id: &ObjectID, version: u64) -> Result<PathBuf, Error> {
        Ok(self.object_dir(object_id)?.join(version.to_string()))
    }

    fn root_versions_path(&self, object_id: &ObjectID) -> Result<PathBuf, Error> {
        Ok(self.object_dir(object_id)?.join(ROOT_VERSIONS_FILE))
    }

    fn checkpoint_versions_path(&self, object_id: &ObjectID) -> Result<PathBuf, Error> {
        Ok(self.object_dir(object_id)?.join(CHECKPOINT_VERSIONS_FILE))
    }

    fn read_bcs_file<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, Error> {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read file {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("failed to decode BCS payload from {}", path.display()))
    }

    fn write_bcs_file<T: serde::Serialize>(&self, path: &Path, value: &T) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let bytes = bcs::to_bytes(value)
            .with_context(|| format!("failed to encode BCS payload for {}", path.display()))?;
        fs::write(path, bytes)
            .with_context(|| format!("failed to write file {}", path.display()))?;
        Ok(())
    }

    fn read_version_mapping(&self, path: &Path) -> Result<BTreeMap<u64, u64>, Error> {
        if !path.exists() {
            return Ok(BTreeMap::new());
        }

        let mut mapping = BTreeMap::new();
        let file = fs::File::open(path)
            .with_context(|| format!("failed to open mapping file {}", path.display()))?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(file);

        for record in reader.records() {
            let record =
                record.with_context(|| format!("failed to read record from {}", path.display()))?;
            if record.len() != 2 {
                return Err(anyhow!(
                    "invalid mapping file {}: expected 2 columns, got {}",
                    path.display(),
                    record.len()
                ));
            }
            let key = record[0]
                .parse::<u64>()
                .with_context(|| format!("failed to parse mapping key in {}", path.display()))?;
            let version = record[1].parse::<u64>().with_context(|| {
                format!("failed to parse mapping version in {}", path.display())
            })?;
            mapping.insert(key, version);
        }

        Ok(mapping)
    }

    fn write_full_version_mapping(
        &self,
        path: &Path,
        mapping: &BTreeMap<u64, u64>,
    ) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let mut content = String::new();
        for (key, version) in mapping {
            content.push_str(&format!("{key},{version}\n"));
        }

        fs::write(path, content)
            .with_context(|| format!("failed to write mapping file {}", path.display()))?;
        Ok(())
    }

    fn load_root_mapping(&self, object_id: &ObjectID) -> Result<(), Error> {
        if self
            .root_versions_map
            .read()
            .unwrap()
            .contains_key(object_id)
        {
            return Ok(());
        }
        let mapping = self.read_version_mapping(&self.root_versions_path(object_id)?)?;
        self.root_versions_map
            .write()
            .unwrap()
            .insert(*object_id, mapping);
        Ok(())
    }

    fn load_checkpoint_mapping(&self, object_id: &ObjectID) -> Result<(), Error> {
        if self
            .checkpoint_versions_map
            .read()
            .unwrap()
            .contains_key(object_id)
        {
            return Ok(());
        }
        let mapping = self.read_version_mapping(&self.checkpoint_versions_path(object_id)?)?;
        self.checkpoint_versions_map
            .write()
            .unwrap()
            .insert(*object_id, mapping);
        Ok(())
    }

    fn update_root_mapping(
        &self,
        object_id: &ObjectID,
        root_version: u64,
        actual_version: u64,
    ) -> Result<(), Error> {
        self.load_root_mapping(object_id)?;
        let mut maps = self.root_versions_map.write().unwrap();
        let mapping = maps.entry(*object_id).or_default();
        mapping.insert(root_version, actual_version);
        self.write_full_version_mapping(&self.root_versions_path(object_id)?, mapping)
    }

    fn update_checkpoint_mapping(
        &self,
        object_id: &ObjectID,
        checkpoint: u64,
        actual_version: u64,
    ) -> Result<(), Error> {
        self.load_checkpoint_mapping(object_id)?;
        let mut maps = self.checkpoint_versions_map.write().unwrap();
        let mapping = maps.entry(*object_id).or_default();
        mapping.insert(checkpoint, actual_version);
        self.write_full_version_mapping(&self.checkpoint_versions_path(object_id)?, mapping)
    }

    fn get_object_by_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        let path = self.object_version_path(object_id, version)?;
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path)
            .map(|object| Some((object, version)))
    }

    fn get_object_by_root_version(
        &self,
        object_id: &ObjectID,
        root_version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.load_root_mapping(object_id)?;
        let maps = self.root_versions_map.read().unwrap();
        let Some(actual_version) = maps
            .get(object_id)
            .and_then(|mapping| mapping.get(&root_version))
            .copied()
        else {
            return Ok(None);
        };
        drop(maps);
        self.get_object_by_version(object_id, actual_version)
    }

    fn get_object_by_checkpoint(
        &self,
        object_id: &ObjectID,
        checkpoint: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.load_checkpoint_mapping(object_id)?;
        let maps = self.checkpoint_versions_map.read().unwrap();
        let Some(actual_version) = maps
            .get(object_id)
            .and_then(|mapping| mapping.get(&checkpoint))
            .copied()
        else {
            return Ok(None);
        };
        drop(maps);
        self.get_object_by_version(object_id, actual_version)
    }

    fn get_chain_id_for_node(&self) -> Result<String, Error> {
        let mapping_file = self.base_path.join(NODE_MAPPING_FILE);
        if !mapping_file.exists() {
            return Err(anyhow!(
                "node mapping file {} does not exist; call setup() first",
                mapping_file.display()
            ));
        }

        let file = fs::File::open(&mapping_file)
            .with_context(|| format!("failed to open {}", mapping_file.display()))?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(file);

        for record in reader.records() {
            let record =
                record.with_context(|| format!("failed to read {}", mapping_file.display()))?;
            if record.len() != 2 {
                return Err(anyhow!(
                    "invalid node mapping file {}: expected 2 columns, got {}",
                    mapping_file.display(),
                    record.len()
                ));
            }
            if record[0].trim() == self.node_key() {
                return Ok(record[1].trim().to_string());
            }
        }

        Err(anyhow!(
            "no node mapping found for '{}' in {}",
            self.node_key(),
            mapping_file.display()
        ))
    }

    fn write_chain_identifier(&self, chain_id: String) -> Result<(), Error> {
        let mapping_file = self.base_path.join(NODE_MAPPING_FILE);
        if let Some(parent) = mapping_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let mut mappings = Vec::<(String, String)>::new();
        if mapping_file.exists() {
            let file = fs::File::open(&mapping_file)
                .with_context(|| format!("failed to open {}", mapping_file.display()))?;
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .trim(csv::Trim::All)
                .from_reader(file);

            for record in reader.records() {
                let record =
                    record.with_context(|| format!("failed to read {}", mapping_file.display()))?;
                if record.len() == 2 {
                    mappings.push((record[0].trim().to_string(), record[1].trim().to_string()));
                }
            }
        }

        let node_key = self.node_key().to_string();
        let mut updated = false;
        for (node, existing_chain_id) in &mut mappings {
            if *node == node_key {
                *existing_chain_id = chain_id.clone();
                updated = true;
            }
        }
        if !updated {
            mappings.push((node_key, chain_id));
        }

        let file = fs::File::create(&mapping_file)
            .with_context(|| format!("failed to create {}", mapping_file.display()))?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);
        for (node, chain_id) in mappings {
            writer
                .write_record([node, chain_id])
                .with_context(|| format!("failed to update {}", mapping_file.display()))?;
        }
        writer
            .flush()
            .with_context(|| format!("failed to flush {}", mapping_file.display()))?;
        Ok(())
    }

    /// Filesystem-specific latest-object helper.
    pub fn get_object_latest(&self, object_id: &ObjectID) -> Result<Option<(Object, u64)>, Error> {
        let object_dir = self.object_dir(object_id)?;
        if !object_dir.exists() {
            return Ok(None);
        }

        let latest_version = fs::read_dir(&object_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                if !entry.file_type().ok()?.is_file() {
                    return None;
                }
                entry.file_name().to_str()?.parse::<u64>().ok()
            })
            .max();

        match latest_version {
            Some(version) => self.get_object_by_version(object_id, version),
            None => Ok(None),
        }
    }

    /// Filesystem-specific owner scan helper.
    pub fn get_objects_by_owner(&self, owner: SuiAddress) -> Result<Vec<Object>, Error> {
        let objects_dir = self.objects_dir()?;
        if !objects_dir.exists() {
            return Ok(Vec::new());
        }

        let mut objects = Vec::new();
        for entry in fs::read_dir(objects_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let object_id_str = entry.file_name().to_string_lossy().to_string();
            let Ok(object_id) = ObjectID::from_hex_literal(&object_id_str) else {
                continue;
            };
            let Some((object, _version)) = self.get_object_latest(&object_id)? else {
                continue;
            };
            let owned_by_address = matches!(object.owner(), Owner::AddressOwner(address) if *address == owner)
                || matches!(
                    object.owner(),
                    Owner::ConsensusAddressOwner { owner: address, .. } if *address == owner
                );
            if owned_by_address {
                objects.push(object);
            }
        }

        Ok(objects)
    }

    /// Filesystem-specific exact-version helper.
    pub fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.get_object_by_version(object_id, version)
    }

    /// Filesystem-specific root-version helper.
    pub fn get_object_at_root_version(
        &self,
        object_id: &ObjectID,
        root_version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.get_object_by_root_version(object_id, root_version)
    }

    /// Filesystem-specific checkpoint helper.
    pub fn get_object_at_checkpoint(
        &self,
        object_id: &ObjectID,
        checkpoint: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.get_object_by_checkpoint(object_id, checkpoint)
    }
}

impl TransactionStore for FileSystemStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("filesystem transaction reads are not implemented in the PR2 slice")
    }
}

impl TransactionStoreWriter for FileSystemStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("filesystem transaction writes are not implemented in the PR2 slice")
    }
}

impl EpochStore for FileSystemStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("filesystem epoch reads are not implemented in the PR2 slice")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("filesystem protocol-config reads are not implemented in the PR2 slice")
    }
}

impl EpochStoreWriter for FileSystemStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("filesystem epoch writes are not implemented in the PR2 slice")
    }
}

impl ObjectStore for FileSystemStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let (result, hit_counter, miss_counter, error_counter) = match key.version_query {
                VersionQuery::Version(version) => (
                    self.get_object_by_version(&key.object_id, version),
                    &self.metrics.obj_version_hit,
                    &self.metrics.obj_version_miss,
                    &self.metrics.obj_version_error,
                ),
                VersionQuery::RootVersion(root_version) => (
                    self.get_object_by_root_version(&key.object_id, root_version),
                    &self.metrics.obj_root_hit,
                    &self.metrics.obj_root_miss,
                    &self.metrics.obj_root_error,
                ),
                VersionQuery::AtCheckpoint(checkpoint) => (
                    self.get_object_by_checkpoint(&key.object_id, checkpoint),
                    &self.metrics.obj_checkpoint_hit,
                    &self.metrics.obj_checkpoint_miss,
                    &self.metrics.obj_checkpoint_error,
                ),
            };

            match result {
                Ok(value) => {
                    if value.is_some() {
                        hit_counter.fetch_add(1, Ordering::Relaxed);
                    } else {
                        miss_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    results.push(value);
                }
                Err(err) => {
                    error_counter.fetch_add(1, Ordering::Relaxed);
                    return Err(err);
                }
            }
        }

        Ok(results)
    }
}

impl ObjectStoreWriter for FileSystemStore {
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), Error> {
        let object_path = self.object_version_path(&key.object_id, actual_version)?;
        self.write_bcs_file(&object_path, &object)?;

        match key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(root_version) => {
                self.update_root_mapping(&key.object_id, root_version, actual_version)?;
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                self.update_checkpoint_mapping(&key.object_id, checkpoint, actual_version)?;
            }
        }

        Ok(())
    }
}

impl CheckpointStore for FileSystemStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        todo!("filesystem checkpoint reads are not implemented in the PR2 slice")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
        todo!("filesystem latest-checkpoint lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("filesystem checkpoint-digest lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("filesystem contents-digest lookups are not implemented in the PR2 slice")
    }
}

impl CheckpointStoreWriter for FileSystemStore {
    fn write_checkpoint(&self, _checkpoint: &FullCheckpointData) -> Result<(), Error> {
        todo!("filesystem checkpoint writes are not implemented in the PR2 slice")
    }
}

impl SetupStore for FileSystemStore {
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        if let Some(chain_id) = chain_id {
            self.write_chain_identifier(chain_id)?;
        }
        Ok(self.get_chain_id_for_node().ok())
    }
}

impl StoreSummary for FileSystemStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "FileSystemStore(node={}, base_path={})",
            self.node.network_name(),
            self.base_path.display()
        )?;
        writeln!(
            writer,
            "  Objects(version): hit={} miss={} error={}",
            self.metrics.obj_version_hit.load(Ordering::Relaxed),
            self.metrics.obj_version_miss.load(Ordering::Relaxed),
            self.metrics.obj_version_error.load(Ordering::Relaxed)
        )?;
        writeln!(
            writer,
            "  Objects(root): hit={} miss={} error={}",
            self.metrics.obj_root_hit.load(Ordering::Relaxed),
            self.metrics.obj_root_miss.load(Ordering::Relaxed),
            self.metrics.obj_root_error.load(Ordering::Relaxed)
        )?;
        writeln!(
            writer,
            "  Objects(checkpoint): hit={} miss={} error={}",
            self.metrics.obj_checkpoint_hit.load(Ordering::Relaxed),
            self.metrics.obj_checkpoint_miss.load(Ordering::Relaxed),
            self.metrics.obj_checkpoint_error.load(Ordering::Relaxed)
        )?;
        Ok(())
    }
}
