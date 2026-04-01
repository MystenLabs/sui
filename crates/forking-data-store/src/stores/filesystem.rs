// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! File-system backed epoch/checkpoint/object store.
//!
//! # Base Directory
//!
//! [`FileSystemStore::base_path`] resolves the store root from
//! `FORKING_DATA_STORE` first, then `SUI_CONFIG_DIR`, `HOME`, or
//! `USERPROFILE`, and finally appends [`DATA_STORE_DIR`].
//!
//! # Intended Directory Structure
//!
//! ```text
//! ~./<store root>/
//!   node_mapping.csv
//!   <chain_id>/
//!     epoch/
//!       <epoch_id>
//!     checkpoint/
//!       latest
//!       checkpoint_digest_index.csv
//!       contents_digest_index.csv
//!       <sequence>.binpb.zst
//!     objects/
//!       <object_id>/
//!         <version>
//!         root_versions
//!         checkpoint_versions
//!     forked_at_<checkpoint>/
//!       epoch/
//!         <epoch_id>
//!       checkpoint/
//!         latest
//!         checkpoint_digest_index.csv
//!         contents_digest_index.csv
//!         <sequence>.binpb.zst
//!       objects/
//!         <object_id>/
//!           <version>
//!           root_versions
//!           checkpoint_versions
//! ```
//!
//! The `checkpoint/` directory stores full checkpoint payloads together with the
//! indexes needed to resolve checkpoint and contents digests back to a sequence
//! number. Stores created for a specific fork session namespace their epoch,
//! checkpoint, and object data under `forked_at_<checkpoint>` so multiple fork
//! sessions can coexist under the same chain ID.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::RwLock,
};

use anyhow::{Context, Error, Result, anyhow};
use prost::Message;
use sui_rpc::{field::FieldMaskTree, merge::Merge};
use sui_types::{
    base_types::ObjectID,
    committee::ProtocolVersion,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    message_envelope::Message as _,
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::ProtocolConfig,
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, LatestObjectStore, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore,
    StoreSummary, VersionQuery, node::Node, normalize_chain_identifier,
};

/// Directory name appended to the configured filesystem store root.
pub const DATA_STORE_DIR: &str = ".forking_data_store";
/// CSV file mapping a logical node name to a concrete chain identifier.
pub const NODE_MAPPING_FILE: &str = "node_mapping.csv";
/// Per-chain epoch storage directory.
pub const EPOCH_DIR: &str = "epoch";
/// Per-chain object storage directory.
pub const OBJECTS_DIR: &str = "objects";
/// Per-chain checkpoint storage directory.
pub const CHECKPOINT_DIR: &str = "checkpoint";
/// File extension used for serialized checkpoint payloads.
pub const CHECKPOINT_FILE_EXTENSION: &str = "binpb.zst";
/// CSV index mapping checkpoint digests to checkpoint sequence numbers.
pub const CHECKPOINT_DIGEST_INDEX_FILE: &str = "checkpoint_digest_index.csv";
/// CSV index mapping checkpoint contents digests to checkpoint sequence numbers.
pub const CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE: &str = "contents_digest_index.csv";
/// Marker file for the latest checkpoint sequence known to the store.
pub const CHECKPOINT_LATEST_FILE: &str = "latest";
/// CSV mapping a root-version bound to the concrete stored object version.
pub const ROOT_VERSIONS_FILE: &str = "root_versions";
/// CSV mapping a checkpoint to the concrete stored object version.
pub const CHECKPOINT_VERSIONS_FILE: &str = "checkpoint_versions";
/// Prefix used for per-fork session directories under a chain ID.
pub const FORK_DIRECTORY_PREFIX: &str = "forked_at_";

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

/// Persistent file-system store rooted at a configurable on-disk path.
#[derive(Debug)]
pub struct FileSystemStore {
    node: Node,
    base_path: PathBuf,
    fork_directory: Option<String>,
    checkpoint_digest_index: RwLock<Option<BTreeMap<String, CheckpointSequenceNumber>>>,
    checkpoint_contents_digest_index: RwLock<Option<BTreeMap<String, CheckpointSequenceNumber>>>,
    root_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
    checkpoint_versions_map: RwLock<BTreeMap<ObjectID, BTreeMap<u64, u64>>>,
}

impl FileSystemStore {
    /// Create a store rooted at the default data-store path.
    pub fn new(node: Node) -> Result<Self, Error> {
        let base_path = Self::base_path()?;
        Ok(Self::new_with_path_unchecked(node, base_path, None))
    }

    /// Create a store rooted at an explicit path.
    pub fn new_with_path(node: Node, full_path: PathBuf) -> Result<Self, Error> {
        Ok(Self::new_with_path_unchecked(node, full_path, None))
    }

    /// Create a store rooted at an explicit path and scoped to a specific fork.
    pub fn new_with_path_for_fork(
        node: Node,
        full_path: PathBuf,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Result<Self, Error> {
        Ok(Self::new_with_path_unchecked(
            node,
            full_path,
            Some(Self::fork_directory_name(forked_at_checkpoint)),
        ))
    }

    /// Create a store rooted at the default path and scoped to a specific fork.
    pub fn new_for_fork(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Result<Self, Error> {
        let base_path = Self::base_path()?;
        Ok(Self::new_with_path_unchecked(
            node,
            base_path,
            Some(Self::fork_directory_name(forked_at_checkpoint)),
        ))
    }

    fn new_with_path_unchecked(
        node: Node,
        full_path: PathBuf,
        fork_directory: Option<String>,
    ) -> Self {
        Self {
            node,
            base_path: full_path,
            fork_directory,
            checkpoint_digest_index: RwLock::new(None),
            checkpoint_contents_digest_index: RwLock::new(None),
            root_versions_map: RwLock::new(BTreeMap::new()),
            checkpoint_versions_map: RwLock::new(BTreeMap::new()),
        }
    }

    /// Resolve the default base path for on-disk storage.
    pub fn base_path() -> Result<PathBuf, Error> {
        let home_dir = std::env::var("FORKING_DATA_STORE")
            .or_else(|_| std::env::var("SUI_CONFIG_DIR"))
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                anyhow!(
                    "cannot determine home directory; define a FORKING_DATA_STORE environment variable"
                )
            })?;
        Ok(PathBuf::from(home_dir).join(DATA_STORE_DIR))
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> sui_types::supported_protocol_versions::Chain {
        self.node.chain()
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Return the configured base path.
    pub fn store_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Return the per-fork directory name for a given fork origin checkpoint.
    pub fn fork_directory_name(forked_at_checkpoint: CheckpointSequenceNumber) -> String {
        format!("{FORK_DIRECTORY_PREFIX}{forked_at_checkpoint}")
    }

    fn node_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.base_path.join(self.get_chain_id_for_node()?))
    }

    fn scoped_chain_dir(&self) -> Result<PathBuf, Error> {
        let node_dir = self.node_dir()?;
        Ok(match &self.fork_directory {
            Some(fork_directory) => node_dir.join(fork_directory),
            None => node_dir,
        })
    }

    fn epoch_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.scoped_chain_dir()?.join(EPOCH_DIR))
    }

    fn objects_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.scoped_chain_dir()?.join(OBJECTS_DIR))
    }

    fn checkpoint_dir(&self) -> Result<PathBuf, Error> {
        Ok(self.scoped_chain_dir()?.join(CHECKPOINT_DIR))
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

    fn checkpoint_file_path(&self, sequence: CheckpointSequenceNumber) -> Result<PathBuf, Error> {
        Ok(self
            .checkpoint_dir()?
            .join(format!("{sequence}.{CHECKPOINT_FILE_EXTENSION}")))
    }

    fn checkpoint_latest_path(&self) -> Result<PathBuf, Error> {
        Ok(self.checkpoint_dir()?.join(CHECKPOINT_LATEST_FILE))
    }

    fn checkpoint_digest_index_path(&self) -> Result<PathBuf, Error> {
        Ok(self.checkpoint_dir()?.join(CHECKPOINT_DIGEST_INDEX_FILE))
    }

    fn checkpoint_contents_digest_index_path(&self) -> Result<PathBuf, Error> {
        Ok(self
            .checkpoint_dir()?
            .join(CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE))
    }

    fn read_bcs_file<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, Error> {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read file: {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("failed to deserialize BCS data from: {}", path.display()))
    }

    fn write_bcs_file<T: serde::Serialize>(&self, path: &Path, data: &T) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        let bytes = bcs::to_bytes(data)
            .with_context(|| format!("failed to serialize BCS data for: {}", path.display()))?;
        fs::write(path, bytes)
            .with_context(|| format!("failed to write file: {}", path.display()))?;
        Ok(())
    }

    fn read_version_mapping(&self, path: &Path) -> Result<BTreeMap<u64, u64>, Error> {
        if !path.exists() {
            return Ok(BTreeMap::new());
        }

        let file = fs::File::open(path)
            .with_context(|| format!("failed to open version mapping file: {}", path.display()))?;
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(file);

        let mut mapping = BTreeMap::new();
        for result in rdr.records() {
            let record = result
                .with_context(|| format!("failed to read CSV record from: {}", path.display()))?;
            if record.len() != 2 {
                return Err(anyhow!(
                    "invalid format in version mapping file {}: expected 2 columns, got {}",
                    path.display(),
                    record.len()
                ));
            }

            let key = record[0].parse().with_context(|| {
                format!(
                    "failed to parse mapping key '{}' in {}",
                    &record[0],
                    path.display()
                )
            })?;
            let version = record[1].parse().with_context(|| {
                format!(
                    "failed to parse object version '{}' in {}",
                    &record[1],
                    path.display()
                )
            })?;
            mapping.insert(key, version);
        }

        Ok(mapping)
    }

    fn write_version_mapping(
        &self,
        path: &Path,
        mapping: &BTreeMap<u64, u64>,
    ) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let mut file = fs::File::create(path).with_context(|| {
            format!("failed to create version mapping file: {}", path.display())
        })?;
        for (key, version) in mapping {
            writeln!(file, "{key},{version}").with_context(|| {
                format!("failed to write version mapping file: {}", path.display())
            })?;
        }

        Ok(())
    }

    fn load_root_mapping(&self, object_id: &ObjectID) -> Result<(), Error> {
        use std::collections::btree_map::Entry;
        let mut guard = self
            .root_versions_map
            .write()
            .expect("root versions lock poisoned");
        if let Entry::Vacant(entry) = guard.entry(*object_id) {
            let mapping = self.read_version_mapping(&self.root_versions_path(object_id)?)?;
            entry.insert(mapping);
        }
        Ok(())
    }

    fn load_checkpoint_mapping(&self, object_id: &ObjectID) -> Result<(), Error> {
        use std::collections::btree_map::Entry;
        let mut guard = self
            .checkpoint_versions_map
            .write()
            .expect("checkpoint versions lock poisoned");
        if let Entry::Vacant(entry) = guard.entry(*object_id) {
            let mapping = self.read_version_mapping(&self.checkpoint_versions_path(object_id)?)?;
            entry.insert(mapping);
        }
        Ok(())
    }

    fn update_root_mapping(
        &self,
        object_id: &ObjectID,
        key: u64,
        version: u64,
    ) -> Result<(), Error> {
        self.load_root_mapping(object_id)?;
        let path = self.root_versions_path(object_id)?;
        let mut guard = self
            .root_versions_map
            .write()
            .expect("root versions lock poisoned");
        let mapping = guard
            .get_mut(object_id)
            .expect("root versions mapping must be loaded");
        mapping.insert(key, version);
        self.write_version_mapping(&path, mapping)
    }

    fn update_checkpoint_mapping(
        &self,
        object_id: &ObjectID,
        key: u64,
        version: u64,
    ) -> Result<(), Error> {
        self.load_checkpoint_mapping(object_id)?;
        let path = self.checkpoint_versions_path(object_id)?;
        let mut guard = self
            .checkpoint_versions_map
            .write()
            .expect("checkpoint versions lock poisoned");
        let mapping = guard
            .get_mut(object_id)
            .expect("checkpoint versions mapping must be loaded");
        mapping.insert(key, version);
        self.write_version_mapping(&path, mapping)
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

        Ok(Some((self.read_bcs_file(&path)?, version)))
    }

    fn get_object_by_root_version(
        &self,
        object_id: &ObjectID,
        max_version: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.load_root_mapping(object_id)?;
        let guard = self
            .root_versions_map
            .read()
            .expect("root versions lock poisoned");
        let Some(actual_version) = guard
            .get(object_id)
            .and_then(|mapping| mapping.get(&max_version).copied())
        else {
            let fallback_version = self
                .scan_object_versions(object_id)?
                .into_iter()
                .filter(|version| *version <= max_version)
                .max();
            return fallback_version.map_or(Ok(None), |version| {
                self.get_object_by_version(object_id, version)
            });
        };

        self.get_object_by_version(object_id, actual_version)
    }

    fn get_object_by_checkpoint(
        &self,
        object_id: &ObjectID,
        checkpoint: u64,
    ) -> Result<Option<(Object, u64)>, Error> {
        self.load_checkpoint_mapping(object_id)?;
        let guard = self
            .checkpoint_versions_map
            .read()
            .expect("checkpoint versions lock poisoned");
        let Some(actual_version) = guard
            .get(object_id)
            .and_then(|mapping| mapping.get(&checkpoint).copied())
        else {
            return Ok(None);
        };

        self.get_object_by_version(object_id, actual_version)
    }

    /// Return the latest locally persisted version of an object, if any.
    ///
    /// This scans the object directory on every call. Acceptable at the current
    /// per-object version count; consider caching max version if this becomes
    /// a hot path.
    pub fn get_object_latest(&self, object_id: &ObjectID) -> Result<Option<(Object, u64)>, Error> {
        let latest_version = self.scan_object_versions(object_id)?.into_iter().max();

        latest_version.map_or(Ok(None), |version| {
            self.get_object_by_version(object_id, version)
        })
    }

    fn scan_object_versions(&self, object_id: &ObjectID) -> Result<Vec<u64>, Error> {
        let object_dir = self.object_dir(object_id)?;
        if !object_dir.exists() {
            return Ok(Vec::new());
        }

        fs::read_dir(&object_dir)
            .with_context(|| format!("failed to read object directory: {}", object_dir.display()))?
            .map(|entry| {
                let entry = entry.with_context(|| {
                    format!(
                        "failed to read object directory entry: {}",
                        object_dir.display()
                    )
                })?;
                Ok(entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u64>().ok()))
            })
            .filter_map(|result| match result {
                Ok(Some(version)) => Some(Ok(version)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn read_digest_index(
        &self,
        path: &Path,
    ) -> Result<BTreeMap<String, CheckpointSequenceNumber>, Error> {
        if !path.exists() {
            return Ok(BTreeMap::new());
        }

        let file = fs::File::open(path)
            .with_context(|| format!("failed to open digest index file: {}", path.display()))?;
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(file);

        let mut mapping = BTreeMap::new();
        for result in rdr.records() {
            let record = result
                .with_context(|| format!("failed to read CSV record from: {}", path.display()))?;
            if record.len() != 2 {
                return Err(anyhow!(
                    "invalid format in digest index file {}: expected 2 columns, got {}",
                    path.display(),
                    record.len()
                ));
            }
            let digest = record[0].trim().to_string();
            let sequence = record[1].parse().with_context(|| {
                format!(
                    "failed to parse sequence '{}' in {}",
                    &record[1],
                    path.display()
                )
            })?;
            mapping.insert(digest, sequence);
        }

        Ok(mapping)
    }

    fn write_digest_index(
        &self,
        path: &Path,
        mapping: &BTreeMap<String, CheckpointSequenceNumber>,
    ) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let file = fs::File::create(path)
            .with_context(|| format!("failed to create digest index file: {}", path.display()))?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);

        for (digest, sequence) in mapping {
            writer
                .write_record([digest.as_str(), &sequence.to_string()])
                .with_context(|| {
                    format!(
                        "failed to write digest index record '{}' -> {}",
                        digest, sequence
                    )
                })?;
        }

        writer
            .flush()
            .with_context(|| format!("failed to flush digest index file: {}", path.display()))?;
        Ok(())
    }

    fn load_checkpoint_digest_index(&self) -> Result<(), Error> {
        if self
            .checkpoint_digest_index
            .read()
            .expect("checkpoint digest index lock poisoned")
            .is_some()
        {
            return Ok(());
        }

        let mapping = self.read_digest_index(&self.checkpoint_digest_index_path()?)?;
        *self
            .checkpoint_digest_index
            .write()
            .expect("checkpoint digest index lock poisoned") = Some(mapping);
        Ok(())
    }

    fn load_checkpoint_contents_digest_index(&self) -> Result<(), Error> {
        if self
            .checkpoint_contents_digest_index
            .read()
            .expect("checkpoint contents index lock poisoned")
            .is_some()
        {
            return Ok(());
        }

        let mapping = self.read_digest_index(&self.checkpoint_contents_digest_index_path()?)?;
        *self
            .checkpoint_contents_digest_index
            .write()
            .expect("checkpoint contents index lock poisoned") = Some(mapping);
        Ok(())
    }

    fn update_checkpoint_digest_index(
        &self,
        digest: CheckpointDigest,
        sequence: CheckpointSequenceNumber,
    ) -> Result<(), Error> {
        self.load_checkpoint_digest_index()?;
        let path = self.checkpoint_digest_index_path()?;
        let mut guard = self
            .checkpoint_digest_index
            .write()
            .expect("checkpoint digest index lock poisoned");
        let mapping = guard
            .as_mut()
            .expect("checkpoint digest index must be loaded");
        mapping.insert(digest.to_string(), sequence);
        self.write_digest_index(&path, mapping)
    }

    fn update_checkpoint_contents_digest_index(
        &self,
        digest: CheckpointContentsDigest,
        sequence: CheckpointSequenceNumber,
    ) -> Result<(), Error> {
        self.load_checkpoint_contents_digest_index()?;
        let path = self.checkpoint_contents_digest_index_path()?;
        let mut guard = self
            .checkpoint_contents_digest_index
            .write()
            .expect("checkpoint contents index lock poisoned");
        let mapping = guard
            .as_mut()
            .expect("checkpoint contents index must be loaded");
        mapping.insert(digest.to_string(), sequence);
        self.write_digest_index(&path, mapping)
    }

    fn read_checkpoint_file(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        let path = self.checkpoint_file_path(sequence)?;
        if !path.exists() {
            return Ok(None);
        }

        let compressed = fs::read(&path)
            .with_context(|| format!("failed to read checkpoint file: {}", path.display()))?;
        let decompressed = zstd::decode_all(&compressed[..])
            .with_context(|| format!("failed to decompress checkpoint file: {}", path.display()))?;
        let proto = sui_rpc::proto::sui::rpc::v2::Checkpoint::decode(&decompressed[..])
            .with_context(|| format!("failed to decode checkpoint proto: {}", path.display()))?;
        let checkpoint = CheckpointData::try_from(&proto)
            .with_context(|| format!("failed to convert checkpoint proto: {}", path.display()))?;
        Ok(Some(checkpoint))
    }

    fn write_checkpoint_file(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        let path = self.checkpoint_file_path(checkpoint.summary.data().sequence_number)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let mut proto = sui_rpc::proto::sui::rpc::v2::Checkpoint::default();
        Merge::merge(&mut proto, checkpoint, &FieldMaskTree::new_wildcard());
        let proto_bytes = proto.encode_to_vec();
        let compressed = zstd::encode_all(&proto_bytes[..], 3)
            .with_context(|| format!("failed to compress checkpoint: {}", path.display()))?;
        fs::write(&path, compressed)
            .with_context(|| format!("failed to write checkpoint file: {}", path.display()))?;
        Ok(())
    }

    fn latest_checkpoint_sequence(&self) -> Result<Option<CheckpointSequenceNumber>, Error> {
        let path = self.checkpoint_latest_path()?;
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).with_context(|| {
            format!("failed to read latest checkpoint file: {}", path.display())
        })?;
        let sequence = contents.trim().parse().with_context(|| {
            format!(
                "failed to parse latest checkpoint sequence '{}' in {}",
                contents.trim(),
                path.display()
            )
        })?;
        Ok(Some(sequence))
    }

    fn update_latest_checkpoint_marker(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<(), Error> {
        let current = self.latest_checkpoint_sequence()?;
        if current.is_some_and(|current| current > sequence) {
            return Ok(());
        }

        let path = self.checkpoint_latest_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }
        fs::write(&path, format!("{sequence}\n")).with_context(|| {
            format!(
                "failed to write latest checkpoint marker '{}'",
                path.display()
            )
        })?;
        Ok(())
    }

    fn get_chain_id_for_node(&self) -> Result<String, Error> {
        let mapping_file = self.base_path.join(NODE_MAPPING_FILE);
        if !mapping_file.exists() {
            return Err(anyhow!(
                "node mapping file not found at {}; file must exist with format: node,chain_id",
                mapping_file.display()
            ));
        }

        let file = fs::File::open(&mapping_file).with_context(|| {
            format!(
                "failed to open node mapping file: {}",
                mapping_file.display()
            )
        })?;

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(file);

        let node_key = match &self.node {
            Node::Mainnet => "mainnet",
            Node::Testnet => "testnet",
            Node::Devnet => "devnet",
            Node::Custom(url) => url.as_str(),
        };

        for result in rdr.records() {
            let record = result.with_context(|| {
                format!("failed to read CSV record from: {}", mapping_file.display())
            })?;

            if record.len() != 2 {
                return Err(anyhow!(
                    "invalid format in node mapping file {}: expected 2 columns, got {}",
                    mapping_file.display(),
                    record.len()
                ));
            }

            if record[0].trim() == node_key {
                return normalize_chain_identifier(record[1].trim()).with_context(|| {
                    format!(
                        "failed to normalize chain identifier '{}' from {}",
                        record[1].trim(),
                        mapping_file.display()
                    )
                });
            }
        }

        Err(anyhow!(
            "no mapping found for node '{}' in mapping file {}",
            node_key,
            mapping_file.display()
        ))
    }

    fn write_chain_identifier(&self, chain_id: String) -> Result<(), Error> {
        let chain_id = normalize_chain_identifier(&chain_id)?;
        let mapping_file = self.base_path.join(NODE_MAPPING_FILE);
        if let Some(parent) = mapping_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let mut mappings = Vec::new();
        if mapping_file.exists() {
            let file = fs::File::open(&mapping_file).with_context(|| {
                format!(
                    "failed to open existing mapping file: {}",
                    mapping_file.display()
                )
            })?;

            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .trim(csv::Trim::All)
                .from_reader(file);

            for result in rdr.records() {
                let record = result.with_context(|| {
                    format!("failed to read CSV record from: {}", mapping_file.display())
                })?;
                if record.len() == 2 {
                    mappings.push((record[0].trim().to_string(), record[1].trim().to_string()));
                }
            }
        }

        let node_key = match &self.node {
            Node::Mainnet => "mainnet",
            Node::Testnet => "testnet",
            Node::Devnet => "devnet",
            Node::Custom(url) => url.as_str(),
        };

        let mut found = false;
        for (node, existing_chain_id) in &mut mappings {
            if node == node_key {
                *existing_chain_id = chain_id.clone();
                found = true;
                break;
            }
        }
        if !found {
            mappings.push((node_key.to_string(), chain_id));
        }

        let file = fs::File::create(&mapping_file).with_context(|| {
            format!("failed to create mapping file: {}", mapping_file.display())
        })?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);

        for (node, chain_id) in mappings {
            writer
                .write_record([node.as_str(), chain_id.as_str()])
                .with_context(|| format!("failed to write mapping record: {node} -> {chain_id}"))?;
        }

        writer
            .flush()
            .with_context(|| format!("failed to flush mapping file: {}", mapping_file.display()))?;
        Ok(())
    }
}

impl EpochStore for FileSystemStore {
    /// Read epoch metadata from the per-chain epoch directory.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        let path = self.epoch_dir()?.join(epoch.to_string());
        if !path.exists() {
            return Ok(None);
        }

        let epoch_data: EpochFileData = self
            .read_bcs_file(&path)
            .with_context(|| format!("failed to load epoch data for epoch {epoch}"))?;
        Ok(Some(epoch_data.into()))
    }

    /// Derive the protocol config from cached epoch metadata.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        Ok(self.epoch_info(epoch)?.map(|epoch_data| {
            ProtocolConfig::get_for_version(
                ProtocolVersion::new(epoch_data.protocol_version),
                self.chain(),
            )
        }))
    }
}

impl EpochStoreWriter for FileSystemStore {
    /// Persist epoch metadata as a BCS-encoded file keyed by epoch number.
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        let path = self.epoch_dir()?.join(epoch.to_string());
        self.write_bcs_file(&path, &EpochFileData::from(epoch_data))
    }
}

impl LatestObjectStore for FileSystemStore {
    fn latest_object(&self, object_id: &ObjectID) -> Result<Option<(Object, u64)>, Error> {
        self.get_object_latest(object_id)
    }
}

impl ObjectStore for FileSystemStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut objects = Vec::with_capacity(keys.len());
        for key in keys {
            let object = match key.version_query {
                VersionQuery::Version(version) => {
                    self.get_object_by_version(&key.object_id, version)
                }
                VersionQuery::RootVersion(max_version) => {
                    self.get_object_by_root_version(&key.object_id, max_version)
                }
                VersionQuery::AtCheckpoint(checkpoint) => {
                    self.get_object_by_checkpoint(&key.object_id, checkpoint)
                }
            }?;
            objects.push(object);
        }
        Ok(objects)
    }
}

impl ObjectStoreWriter for FileSystemStore {
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), Error> {
        let path = self.object_version_path(&key.object_id, actual_version)?;
        self.write_bcs_file(&path, &object)?;

        match key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(max_version) => {
                self.update_root_mapping(&key.object_id, max_version, actual_version)?;
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                self.update_checkpoint_mapping(&key.object_id, checkpoint, actual_version)?;
            }
        }

        Ok(())
    }
}

impl CheckpointStore for FileSystemStore {
    /// Read a checkpoint payload from its compressed sequence-number file.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        self.read_checkpoint_file(sequence)
    }

    /// Resolve the latest checkpoint through the persisted marker file.
    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        let Some(sequence) = self.latest_checkpoint_sequence()? else {
            return Ok(None);
        };
        self.get_checkpoint_by_sequence_number(sequence)
    }

    /// Resolve a checkpoint digest through the on-disk digest index.
    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        self.load_checkpoint_digest_index()?;
        Ok(self
            .checkpoint_digest_index
            .read()
            .expect("checkpoint digest index lock poisoned")
            .as_ref()
            .and_then(|mapping| mapping.get(&digest.to_string()).copied()))
    }

    /// Resolve a contents digest through the on-disk digest index.
    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        self.load_checkpoint_contents_digest_index()?;
        Ok(self
            .checkpoint_contents_digest_index
            .read()
            .expect("checkpoint contents index lock poisoned")
            .as_ref()
            .and_then(|mapping| mapping.get(&digest.to_string()).copied()))
    }
}

impl CheckpointStoreWriter for FileSystemStore {
    /// Persist the checkpoint payload and keep the reverse indexes in sync.
    fn write_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        self.write_checkpoint_file(checkpoint)?;
        self.update_checkpoint_digest_index(
            checkpoint.summary.data().digest(),
            checkpoint.summary.data().sequence_number,
        )?;
        self.update_checkpoint_contents_digest_index(
            *checkpoint.contents.digest(),
            checkpoint.summary.data().sequence_number,
        )?;
        self.update_latest_checkpoint_marker(checkpoint.summary.data().sequence_number)
    }
}

impl SetupStore for FileSystemStore {
    /// Persist or recover the short chain identifier used for the per-chain directory.
    fn setup(&self, chain_id: Option<String>) -> Result<Option<String>, Error> {
        if let Some(chain_id) = chain_id {
            let chain_id = normalize_chain_identifier(&chain_id)?;
            self.write_chain_identifier(chain_id.clone())?;
            return Ok(Some(chain_id));
        }

        let chain_id = self.get_chain_id_for_node().ok();
        if let Some(chain_id) = chain_id.clone() {
            // Rewriting the mapping self-heals legacy entries that stored the long digest form.
            self.write_chain_identifier(chain_id)?;
        }
        Ok(chain_id)
    }
}

impl StoreSummary for FileSystemStore {
    /// Print the store location for debugging and test output.
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "FileSystemStore(node={}, base_path={}, fork_directory={})",
            self.node.network_name(),
            self.base_path.display(),
            self.fork_directory.as_deref().unwrap_or("<shared>")
        )?;
        Ok(())
    }
}
