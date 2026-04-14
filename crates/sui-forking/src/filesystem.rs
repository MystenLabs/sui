// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Folder structure:
//! {base_path}/{network_name}/forked_at_{checkpoint}/
//!     - objects/
//!         - {object_id}/
//!            - latest                  (text: latest persisted version number)
//!            - {version}                (BCS-encoded Object)
//!     - checkpoints/
//!         - latest                     (text: highest persisted sequence number)
//!         - {seq}/
//!             - summary                (BCS-encoded CertifiedCheckpointSummary)
//!             - contents               (BCS-encoded CheckpointContents)
//!         - digest_index               ("{checkpoint_digest} {seq}\n", append-only)
//!         - contents_digest_index      ("{contents_digest} {seq}\n", append-only)
//!     - transactions/
//!         - {tx_digest}/
//!             - data                   (BCS-encoded Transaction envelope)
//!             - effects                (BCS-encoded TransactionEffects)
//!             - events                 (BCS-encoded TransactionEvents)

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;

use sui_types::base_types::ObjectID;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use sui_types::transaction::VerifiedTransaction;

use crate::Node;

/// Directory name appended to the configured filesystem store root.
const DATA_STORE_DIR: &str = ".forking_data_store";
/// Per-chain object storage directory.
const OBJECTS_DIR: &str = "objects";
/// Per-chain checkpoint storage directory.
const CHECKPOINTS_DIR: &str = "checkpoints";
/// Per-chain transaction storage directory.
const TRANSACTIONS_DIR: &str = "transactions";
/// Filename for the BCS-encoded transaction data within a transaction directory.
const TX_DATA_FILE: &str = "data";
/// Filename for the BCS-encoded transaction effects within a transaction directory.
const TX_EFFECTS_FILE: &str = "effects";
/// Filename for the BCS-encoded transaction events within a transaction directory.
const TX_EVENTS_FILE: &str = "events";
/// Filename for the BCS-encoded checkpoint summary within a checkpoint sequence directory.
const CHECKPOINT_SUMMARY_FILE: &str = "summary";
/// Filename for the BCS-encoded checkpoint contents within a checkpoint sequence directory.
const CHECKPOINT_CONTENTS_FILE: &str = "contents";
/// Append-only index file mapping checkpoint digest to sequence number.
const CHECKPOINT_DIGEST_INDEX_FILE: &str = "digest_index";
/// Append-only index file mapping checkpoint contents digest to sequence number.
const CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE: &str = "contents_digest_index";
/// Marker file for the latest checkpoint sequence known to the store.
const LATEST_FILE: &str = "latest";

/// Local filesystem-backed store for Sui data.
pub(crate) struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    /// Create a new filesystem store rooted at
    /// `{base_path}/{network_name}/forked_at_{checkpoint}`.
    pub(crate) fn new(
        node: &Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Result<Self, Error> {
        let root = Self::base_path()?
            .join(node.network_name())
            .join(format!("forked_at_{}", forked_at_checkpoint));
        Ok(Self { root })
    }

    /// Create a filesystem store with an explicit root directory.
    #[cfg(test)]
    pub(crate) fn new_with_root(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve the default base path for on-disk storage.
    pub(crate) fn base_path() -> Result<PathBuf, Error> {
        let home_dir = std::env::var("FORKING_DATA_STORE")
            .or_else(|_| std::env::var("SUI_CONFIG_DIR"))
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                anyhow!(
                    "Cannot determine home directory. Define a FORKING_DATA_STORE environment variable"
                )
            })?;
        Ok(PathBuf::from(home_dir).join(DATA_STORE_DIR))
    }

    /// Return the directory path for storing objects data.
    fn objects_dir(&self) -> PathBuf {
        self.root.join(OBJECTS_DIR)
    }

    /// Return the directory path for storing checkpoint data.
    fn checkpoints_dir(&self) -> PathBuf {
        self.root.join(CHECKPOINTS_DIR)
    }

    /// Return the directory path for storing transaction data.
    fn transactions_dir(&self) -> PathBuf {
        self.root.join(TRANSACTIONS_DIR)
    }

    /// Return the directory for a specific transaction.
    fn transaction_dir(&self, digest: &TransactionDigest) -> PathBuf {
        self.transactions_dir().join(digest.to_string())
    }

    /// Persist a verified transaction to disk under `transactions/{digest}/data`. The underlying
    /// `Transaction` envelope is what gets serialized; the verified marker is reapplied on read.
    pub(crate) fn write_transaction(
        &self,
        digest: &TransactionDigest,
        transaction: &VerifiedTransaction,
    ) -> Result<(), Error> {
        let path = self.transaction_dir(digest).join(TX_DATA_FILE);
        self.write_bcs_file(&path, transaction.inner())
    }

    /// Persist transaction effects to disk under `transactions/{digest}/effects`.
    pub(crate) fn write_transaction_effects(
        &self,
        digest: &TransactionDigest,
        effects: &TransactionEffects,
    ) -> Result<(), Error> {
        let path = self.transaction_dir(digest).join(TX_EFFECTS_FILE);
        self.write_bcs_file(&path, effects)
    }

    /// Persist transaction events to disk under `transactions/{digest}/events`.
    pub(crate) fn write_transaction_events(
        &self,
        digest: &TransactionDigest,
        events: &TransactionEvents,
    ) -> Result<(), Error> {
        let path = self.transaction_dir(digest).join(TX_EVENTS_FILE);
        self.write_bcs_file(&path, events)
    }

    /// Read a previously persisted transaction. Returns `None` if the transaction is not on disk.
    pub(crate) fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Error> {
        let path = self.transaction_dir(digest).join(TX_DATA_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let inner: Transaction = self.read_bcs_file(&path)?;
        Ok(Some(VerifiedTransaction::new_unchecked(inner)))
    }

    /// Read previously persisted transaction effects. Returns `None` if not on disk.
    pub(crate) fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>, Error> {
        let path = self.transaction_dir(digest).join(TX_EFFECTS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
    }

    /// Read previously persisted transaction events. Returns `None` if not on disk.
    pub(crate) fn get_transaction_events(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionEvents>, Error> {
        let path = self.transaction_dir(digest).join(TX_EVENTS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
    }

    /// Get the latest object version available on disk for the given object ID.
    pub(crate) fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        let object_dir = self.objects_dir().join(object_id.to_string());

        if !object_dir.exists() {
            return Ok(None);
        }

        let latest_version = self.read_latest_file(&object_dir)?;
        let version_file = object_dir.join(latest_version.to_string());
        self.read_bcs_file(&version_file).map(Some)
    }

    /// Get the object at the given version for the given object ID. Returns `None` if the version
    /// file does not exist on disk.
    pub(crate) fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        let object_dir = self.objects_dir().join(object_id.to_string());
        let version_file = object_dir.join(version.to_string());

        if !version_file.exists() {
            return Ok(None);
        }

        self.read_bcs_file(&version_file).map(Some)
    }

    /// Write the given object to disk under the objects directory, using the object ID and version
    /// as the path. It will also update the latest file to point to this version.
    pub(crate) fn write_object(&self, object: &Object) -> anyhow::Result<()> {
        let object_dir = self.objects_dir().join(object.id().to_string());
        let version = object.version().value();
        let version_file = object_dir.join(version.to_string());
        self.write_bcs_file(&version_file, object)?;

        let latest_version = if object_dir.join(LATEST_FILE).exists() {
            std::cmp::max(self.read_latest_file(&object_dir)?, version)
        } else {
            version
        };
        let latest_file = object_dir.join(LATEST_FILE);
        fs::write(latest_file, latest_version.to_string())
            .with_context(|| format!("Failed to write latest file for object {}", object.id()))
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub(crate) fn get_highest_checkpoint_sequence_number(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        let checkpoint_dir = self.checkpoints_dir();

        anyhow::ensure!(
            checkpoint_dir.exists(),
            "Checkpoint directory does not exist: {}",
            checkpoint_dir.display()
        );

        self.read_latest_file(&checkpoint_dir)
    }

    /// Path to the per-sequence directory holding `summary` and `contents`.
    fn checkpoint_seq_dir(&self, sequence: CheckpointSequenceNumber) -> PathBuf {
        self.checkpoints_dir().join(sequence.to_string())
    }

    fn checkpoint_digest_index_path(&self) -> PathBuf {
        self.checkpoints_dir().join(CHECKPOINT_DIGEST_INDEX_FILE)
    }

    fn checkpoint_contents_digest_index_path(&self) -> PathBuf {
        self.checkpoints_dir()
            .join(CHECKPOINT_CONTENTS_DIGEST_INDEX_FILE)
    }

    /// Persist a checkpoint summary to `checkpoints/{seq}/summary`, append the
    /// checkpoint digest to the digest index, and bump the `latest` marker if
    /// `seq` is strictly higher than the currently recorded value.
    pub(crate) fn write_checkpoint_summary(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> anyhow::Result<()> {
        let sequence = checkpoint.data().sequence_number;
        let path = self.checkpoint_seq_dir(sequence).join(CHECKPOINT_SUMMARY_FILE);
        self.write_bcs_file(&path, checkpoint.inner())?;

        self.update_latest_checkpoint_marker(sequence)?;
        append_index_line(
            &self.checkpoint_digest_index_path(),
            &checkpoint.digest().to_string(),
            sequence,
        )
    }

    /// Persist checkpoint contents to `checkpoints/{seq}/contents` and append
    /// the contents digest to the contents digest index.
    pub(crate) fn write_checkpoint_contents(
        &self,
        sequence: CheckpointSequenceNumber,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        let path = self
            .checkpoint_seq_dir(sequence)
            .join(CHECKPOINT_CONTENTS_FILE);
        self.write_bcs_file(&path, contents)?;

        append_index_line(
            &self.checkpoint_contents_digest_index_path(),
            &contents.digest().to_string(),
            sequence,
        )
    }

    /// Read a previously persisted checkpoint summary. Returns `None` if no
    /// summary file exists for the given sequence.
    pub(crate) fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let path = self.checkpoint_seq_dir(sequence).join(CHECKPOINT_SUMMARY_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let certified: CertifiedCheckpointSummary = self.read_bcs_file(&path)?;
        Ok(Some(VerifiedCheckpoint::new_unchecked(certified)))
    }

    /// Read previously persisted checkpoint contents. Returns `None` if no
    /// contents file exists for the given sequence.
    pub(crate) fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        let path = self
            .checkpoint_seq_dir(sequence)
            .join(CHECKPOINT_CONTENTS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
    }

    /// Resolve a checkpoint by its summary digest via the digest index.
    pub(crate) fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        match lookup_index(&self.checkpoint_digest_index_path(), &digest.to_string())? {
            Some(sequence) => self.get_checkpoint_by_sequence_number(sequence),
            None => Ok(None),
        }
    }

    /// Resolve checkpoint contents by their digest via the contents digest index.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        match lookup_index(
            &self.checkpoint_contents_digest_index_path(),
            &digest.to_string(),
        )? {
            Some(sequence) => self.get_checkpoint_contents_by_sequence_number(sequence),
            None => Ok(None),
        }
    }

    /// Return the checkpoint at the highest persisted sequence number, or
    /// `None` if nothing has been persisted yet.
    pub(crate) fn get_highest_verified_checkpoint(
        &self,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let checkpoints_dir = self.checkpoints_dir();
        if !checkpoints_dir.join(LATEST_FILE).exists() {
            return Ok(None);
        }
        let sequence = self.read_latest_file(&checkpoints_dir)?;
        self.get_checkpoint_by_sequence_number(sequence)
    }

    /// Update the `checkpoints/latest` marker if `sequence` is higher than the
    /// currently recorded value, creating the file (and parent directory) if
    /// this is the first checkpoint being persisted.
    fn update_latest_checkpoint_marker(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        let dir = self.checkpoints_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
        let path = dir.join(LATEST_FILE);
        let current = if path.exists() {
            Some(self.read_latest_file(&dir)?)
        } else {
            None
        };
        if current.map_or(true, |c| sequence > c) {
            fs::write(&path, sequence.to_string()).with_context(|| {
                format!("Failed to write latest checkpoint marker: {}", path.display())
            })?;
        }
        Ok(())
    }

    fn read_bcs_file<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, Error> {
        let bytes =
            fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("Failed to deserialize BCS data from: {}", path.display()))
    }

    fn write_bcs_file<T: serde::Serialize>(&self, path: &Path, data: &T) -> Result<(), Error> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        let bytes = bcs::to_bytes(data)
            .with_context(|| format!("Failed to serialize BCS data for: {}", path.display()))?;
        fs::write(path, bytes)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        Ok(())
    }

    /// Read the latest file that contains a number representing the latest checkpoint sequence or
    /// object version.
    fn read_latest_file(&self, dir: &Path) -> Result<u64, Error> {
        let latest_path = dir.join(LATEST_FILE);
        if !latest_path.exists() {
            bail!("Latest file not found in directory: {}", dir.display());
        }
        let content = fs::read_to_string(&latest_path)
            .with_context(|| format!("Failed to read latest file: {}", latest_path.display()))?;
        content
            .trim()
            .parse::<u64>()
            .with_context(|| format!("Failed to parse latest file: {}", latest_path.display()))
    }
}

/// Append a single `{key} {value}\n` line to `path`, creating the file and
/// any missing parent directories. Uses `O_APPEND` so concurrent appends of
/// short lines remain atomic under POSIX semantics.
fn append_index_line(
    path: &Path,
    key: &str,
    value: CheckpointSequenceNumber,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open index file: {}", path.display()))?;
    writeln!(file, "{} {}", key, value)
        .with_context(|| format!("Failed to append to index file: {}", path.display()))
}

/// Scan a space-delimited index file for `key`. Returns the value from the
/// last matching line (last-wins), so re-appending the same key is idempotent.
fn lookup_index(
    path: &Path,
    key: &str,
) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read index file: {}", path.display()))?;
    let mut found = None;
    for line in content.lines() {
        let Some((k, v)) = line.split_once(' ') else {
            continue;
        };
        if k == key {
            found = Some(
                v.trim()
                    .parse::<CheckpointSequenceNumber>()
                    .with_context(|| format!("Failed to parse index entry: {}", path.display()))?,
            );
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEvents;
    use sui_types::object::MoveObject;
    use sui_types::object::Object;
    use sui_types::object::ObjectInner;
    use sui_types::object::Owner;

    use super::*;

    fn test_store() -> (tempfile::TempDir, FilesystemStore) {
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let store = FilesystemStore::new_with_root(dir.path().to_path_buf());
        (dir, store)
    }

    fn make_object(id: ObjectID, version: u64) -> Object {
        let move_obj = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, 1_000_000);
        ObjectInner {
            owner: Owner::Immutable,
            data: sui_types::object::Data::Move(move_obj),
            previous_transaction: TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    #[test]
    fn test_write_and_read_latest_object() {
        let (_dir, store) = test_store();
        let id = ObjectID::random();
        let obj = make_object(id, 5);

        store.write_object(&obj).unwrap();
        let loaded = store.get_latest_object(&id).unwrap();
        assert_eq!(loaded.clone().unwrap(), obj);
        assert_eq!(loaded.unwrap().version(), SequenceNumber::from_u64(5));
    }

    #[test]
    fn test_write_and_read_object_at_version() {
        let (_dir, store) = test_store();
        let id = ObjectID::random();
        let obj = make_object(id, 5);

        store.write_object(&obj).unwrap();
        let loaded = store.get_object_at_version(&id, 5).unwrap();
        assert_eq!(loaded.unwrap(), obj);
    }

    #[test]
    fn test_get_latest_object_returns_none_for_unknown_id() {
        let (_dir, store) = test_store();
        let result = store.get_latest_object(&ObjectID::random()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_object_at_version_returns_none_for_unknown_version() {
        let (_dir, store) = test_store();
        let id = ObjectID::random();
        let obj = make_object(id, 5);
        store.write_object(&obj).unwrap();

        let result = store.get_object_at_version(&id, 99).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_latest_tracks_highest_written_version() {
        let (_dir, store) = test_store();
        let id = ObjectID::random();

        let v1 = make_object(id, 1);
        let v3 = make_object(id, 3);
        store.write_object(&v1).unwrap();
        store.write_object(&v3).unwrap();

        let latest = store.get_latest_object(&id).unwrap().unwrap();
        assert_eq!(latest, v3);

        // v1 is still accessible by version
        let old = store.get_object_at_version(&id, 1).unwrap().unwrap();
        assert_eq!(old, v1);
    }

    #[test]
    fn test_get_highest_checkpoint_errors_when_dir_missing() {
        let (_dir, store) = test_store();
        let err = store.get_highest_checkpoint_sequence_number().unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_get_highest_checkpoint_errors_when_latest_file_missing() {
        let (_dir, store) = test_store();
        fs::create_dir_all(store.checkpoints_dir()).unwrap();
        let err = store.get_highest_checkpoint_sequence_number().unwrap_err();
        assert!(err.to_string().contains("Latest file not found"));
    }

    #[test]
    fn test_write_and_read_transaction_effects() {
        let (_dir, store) = test_store();
        let digest = TransactionDigest::random();
        let effects = TransactionEffects::default();

        store.write_transaction_effects(&digest, &effects).unwrap();
        let loaded = store.get_transaction_effects(&digest).unwrap();
        assert_eq!(loaded.unwrap(), effects);
    }

    #[test]
    fn test_write_and_read_transaction_events() {
        let (_dir, store) = test_store();
        let digest = TransactionDigest::random();
        let events = TransactionEvents { data: vec![] };

        store.write_transaction_events(&digest, &events).unwrap();
        let loaded = store.get_transaction_events(&digest).unwrap();
        assert_eq!(loaded.unwrap(), events);
    }

    #[test]
    fn test_get_transaction_returns_none_for_unknown_digest() {
        let (_dir, store) = test_store();
        let digest = TransactionDigest::random();

        assert!(store.get_transaction(&digest).unwrap().is_none());
        assert!(store.get_transaction_effects(&digest).unwrap().is_none());
        assert!(store.get_transaction_events(&digest).unwrap().is_none());
    }

    fn build_checkpoint(sequence: u64) -> (VerifiedCheckpoint, CheckpointContents) {
        let data =
            sui_types::test_checkpoint_data_builder::TestCheckpointBuilder::new(sequence)
                .build_checkpoint();
        let checkpoint = VerifiedCheckpoint::new_unchecked(data.summary);
        (checkpoint, data.contents)
    }

    #[test]
    fn test_write_and_read_checkpoint_by_sequence_and_digest() {
        let (_dir, store) = test_store();
        let (checkpoint, contents) = build_checkpoint(7);
        let sequence = checkpoint.data().sequence_number;

        store.write_checkpoint_summary(&checkpoint).unwrap();
        store.write_checkpoint_contents(sequence, &contents).unwrap();

        let by_seq = store
            .get_checkpoint_by_sequence_number(sequence)
            .unwrap()
            .unwrap();
        assert_eq!(by_seq.data(), checkpoint.data());

        let contents_by_seq = store
            .get_checkpoint_contents_by_sequence_number(sequence)
            .unwrap()
            .unwrap();
        assert_eq!(contents_by_seq.digest(), contents.digest());

        let by_digest = store
            .get_checkpoint_by_digest(checkpoint.digest())
            .unwrap()
            .unwrap();
        assert_eq!(by_digest.data(), checkpoint.data());

        let contents_by_digest = store
            .get_checkpoint_contents_by_digest(contents.digest())
            .unwrap()
            .unwrap();
        assert_eq!(contents_by_digest.digest(), contents.digest());
    }

    #[test]
    fn test_latest_checkpoint_tracks_highest_sequence() {
        let (_dir, store) = test_store();
        let (low, _) = build_checkpoint(3);
        let (high, _) = build_checkpoint(9);

        // Write out-of-order: the `latest` marker must still resolve to the
        // highest sequence that has been persisted.
        store.write_checkpoint_summary(&high).unwrap();
        store.write_checkpoint_summary(&low).unwrap();

        let highest = store.get_highest_verified_checkpoint().unwrap().unwrap();
        assert_eq!(highest.data().sequence_number, 9);
    }

    #[test]
    fn test_checkpoint_lookups_return_none_when_missing() {
        let (_dir, store) = test_store();
        let (checkpoint, contents) = build_checkpoint(1);

        assert!(store.get_checkpoint_by_sequence_number(1).unwrap().is_none());
        assert!(
            store
                .get_checkpoint_contents_by_sequence_number(1)
                .unwrap()
                .is_none()
        );
        assert!(store.get_checkpoint_by_digest(checkpoint.digest()).unwrap().is_none());
        assert!(
            store
                .get_checkpoint_contents_by_digest(contents.digest())
                .unwrap()
                .is_none()
        );
        assert!(store.get_highest_verified_checkpoint().unwrap().is_none());
    }
}
