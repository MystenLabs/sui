// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Folder structure:
//! {base_path}/{network_name}/forked_at_{checkpoint}/
//!     - objects/
//!         - {object_id}/
//!            - latest (contains the latest version number)
//!            - {version} (contains the object data in BCS format)
//!      - checkpoints/
//!          - latest (contains the latest checkpoint sequence number)
//!          - {checkpoint} (contains the checkpoint data in BCS format)
//!      - transactions/
//!          - {tx_digest}/
//!              - data    (BCS-encoded Transaction envelope)
//!              - effects (BCS-encoded TransactionEffects)
//!              - events  (BCS-encoded TransactionEvents)

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;

use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
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
}
