// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Folder structure:
//! {base_path}/{network_name}/forked_at_{checkpoint}/
//!     - objects/
//!         - {object_id}/
//!            - latest                  (text: latest persisted version number)
//!            - removed                 (text marker: removed kind + object ref)
//!            - {version}                (BCS-encoded Object)
//!     - indices/
//!         - owned_objects              (BCS-encoded `Vec<OwnedObjectEntry>`)
//!     - checkpoints/
//!         - latest                     (text: highest persisted sequence number)
//!         - {seq}/
//!             - summary                (BCS-encoded CertifiedCheckpointSummary)
//!         - contents/
//!             - {contents_digest}      (BCS-encoded CheckpointContents)
//!         - digest_index               ("{checkpoint_digest} {seq}\n", append-only)
//!     - transactions/
//!         - {tx_digest}/
//!             - data                   (BCS-encoded Transaction envelope)
//!             - effects                (BCS-encoded TransactionEffects)
//!             - events                 (BCS-encoded TransactionEvents)

use std::fs;
use std::io::ErrorKind;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Error;
use anyhow::anyhow;
use anyhow::bail;

use move_core_types::language_storage::StructTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::transaction::Transaction;
use sui_types::transaction::VerifiedTransaction;

use crate::Node;

/// Directory name appended to the configured filesystem store root.
const DATA_STORE_DIR: &str = ".forking_data_store";
/// Per-chain object storage directory.
const OBJECTS_DIR: &str = "objects";
/// Per-chain secondary indices directory.
const INDICES_DIR: &str = "indices";
/// Per-chain checkpoint storage directory.
const CHECKPOINTS_DIR: &str = "checkpoints";
/// Per-chain transaction storage directory.
const TRANSACTIONS_DIR: &str = "transactions";
/// Filename marking the current local removal state for an object.
const REMOVED_FILE: &str = "removed";
/// BCS-encoded owned-object index filename.
const OWNED_OBJECTS_INDEX_FILE: &str = "owned_objects";
/// Filename for the BCS-encoded transaction data within a transaction directory.
const TX_DATA_FILE: &str = "data";
/// Filename for the BCS-encoded transaction effects within a transaction directory.
const TX_EFFECTS_FILE: &str = "effects";
/// Filename for the BCS-encoded transaction events within a transaction directory.
const TX_EVENTS_FILE: &str = "events";
/// Filename for the checkpoint sequence number within a transaction directory.
const TX_CHECKPOINT_FILE: &str = "checkpoint";
/// Filename for the BCS-encoded checkpoint summary within a checkpoint sequence directory.
const CHECKPOINT_SUMMARY_FILE: &str = "summary";
/// Subdirectory for content-addressed checkpoint contents files.
const CHECKPOINT_CONTENTS_DIR: &str = "contents";
/// Append-only index file mapping checkpoint digest to sequence number.
const CHECKPOINT_DIGEST_INDEX_FILE: &str = "digest_index";
/// Marker file for the latest checkpoint sequence known to the store.
const LATEST_FILE: &str = "latest";

/// Current-state removal kind for an object affected by local execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RemovedObjectKind {
    Deleted,
    Wrapped,
}

impl RemovedObjectKind {
    fn marker_text(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::Wrapped => "wrapped",
        }
    }

    fn from_marker_text(value: &str) -> anyhow::Result<Self> {
        match value {
            "deleted" => Ok(Self::Deleted),
            "wrapped" => Ok(Self::Wrapped),
            _ => bail!("Unknown object removal marker kind: {value}"),
        }
    }
}

/// Index entry for a live address-owned object.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct OwnedObjectEntry {
    pub(crate) owner: SuiAddress,
    pub(crate) object_id: ObjectID,
    pub(crate) version: SequenceNumber,
    pub(crate) object_type: StructTag,
    pub(crate) balance: Option<u64>,
}

impl OwnedObjectEntry {
    fn from_object(object: &Object) -> Option<Self> {
        let Owner::AddressOwner(owner) = &object.owner else {
            return None;
        };
        Some(Self {
            owner: *owner,
            object_id: object.id(),
            version: object.version(),
            object_type: object.struct_tag()?,
            balance: object.as_coin_maybe().map(|coin| coin.value()),
        })
    }
}

/// Local filesystem-backed store for Sui data.
#[derive(Clone)]
pub(crate) struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    /// Create a new filesystem store rooted at
    /// `{base_path}/{network_name}/forked_at_{checkpoint}`.
    pub(crate) fn new(
        node: &Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        data_dir: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let base = match data_dir {
            Some(dir) => dir,
            None => Self::base_path()?,
        };
        let root = base
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

    /// Return the directory path for secondary indices.
    fn indices_dir(&self) -> PathBuf {
        self.root.join(INDICES_DIR)
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
    fn tx_dir(&self, digest: &TransactionDigest) -> PathBuf {
        self.transactions_dir().join(digest.to_string())
    }

    /// Return the file path for the owned-object index.
    fn owned_objects_index_path(&self) -> PathBuf {
        self.indices_dir().join(OWNED_OBJECTS_INDEX_FILE)
    }

    /// Persist a verified transaction to disk under `transactions/{digest}/data`. The underlying
    /// `Transaction` envelope is what gets serialized; the verified marker is reapplied on read.
    pub(crate) fn write_transaction(
        &self,
        digest: &TransactionDigest,
        transaction: &VerifiedTransaction,
    ) -> Result<(), Error> {
        let path = self.tx_dir(digest).join(TX_DATA_FILE);
        self.write_bcs_file(&path, transaction.inner())
    }

    /// Persist transaction effects to disk under `transactions/{digest}/effects`.
    pub(crate) fn write_transaction_effects(
        &self,
        digest: &TransactionDigest,
        effects: &TransactionEffects,
    ) -> Result<(), Error> {
        let path = self.tx_dir(digest).join(TX_EFFECTS_FILE);
        self.write_bcs_file(&path, effects)
    }

    /// Persist transaction events to disk under `transactions/{digest}/events`.
    pub(crate) fn write_transaction_events(
        &self,
        digest: &TransactionDigest,
        events: &TransactionEvents,
    ) -> Result<(), Error> {
        let path = self.tx_dir(digest).join(TX_EVENTS_FILE);
        self.write_bcs_file(&path, events)
    }

    /// Read a previously persisted transaction. Returns `None` if the transaction is not on disk.
    pub(crate) fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Error> {
        let path = self.tx_dir(digest).join(TX_DATA_FILE);
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
        let path = self.tx_dir(digest).join(TX_EFFECTS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
    }

    /// Persist the checkpoint sequence number that finalized a transaction.
    pub(crate) fn write_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<(), Error> {
        let path = self.tx_dir(digest).join(TX_CHECKPOINT_FILE);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        fs::write(&path, checkpoint.to_string())
            .with_context(|| format!("Failed to write checkpoint file: {}", path.display()))
    }

    /// Read the checkpoint sequence number for a previously persisted transaction.
    pub(crate) fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        let path = self.tx_dir(digest).join(TX_CHECKPOINT_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read checkpoint file: {}", path.display()))?;
        let seq = content
            .trim()
            .parse::<u64>()
            .with_context(|| format!("Failed to parse checkpoint file: {}", path.display()))?;
        Ok(Some(seq))
    }

    /// Read previously persisted transaction events. Returns `None` if not on disk.
    pub(crate) fn get_transaction_events(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionEvents>, Error> {
        let path = self.tx_dir(digest).join(TX_EVENTS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
    }

    /// Get the latest object version available on disk for the given object ID.
    pub(crate) fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        if self.is_object_currently_removed(object_id)? {
            return Ok(None);
        }

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

    /// Mark an object as deleted by local execution. Historical version files remain on disk and
    /// can still be read by exact version, but current reads must not resurrect the object.
    pub(crate) fn mark_object_deleted(&self, object_ref: &ObjectRef) -> anyhow::Result<()> {
        self.write_object_removed_marker(RemovedObjectKind::Deleted, object_ref)
    }

    /// Mark an object as wrapped by local execution. Historical version files remain on disk and
    /// can still be read by exact version; a later live write can clear this marker.
    pub(crate) fn mark_object_wrapped(&self, object_ref: &ObjectRef) -> anyhow::Result<()> {
        if self.is_object_deleted(&object_ref.0)? {
            return Ok(());
        }
        self.write_object_removed_marker(RemovedObjectKind::Wrapped, object_ref)
    }

    /// Clear the local deletion marker. Normal live writes do not call this because post-fork
    /// deletions are terminal; it is available for tests and explicit cache repair.
    pub(crate) fn clear_object_deleted(&self, object_id: &ObjectID) -> anyhow::Result<()> {
        self.clear_object_removed_marker_kind(object_id, RemovedObjectKind::Deleted)
    }

    /// Clear the local wrapped marker for an object that has become live again.
    pub(crate) fn clear_object_wrapped(&self, object_id: &ObjectID) -> anyhow::Result<()> {
        self.clear_object_removed_marker_kind(object_id, RemovedObjectKind::Wrapped)
    }

    /// Return whether local execution has deleted the object.
    pub(crate) fn is_object_deleted(&self, object_id: &ObjectID) -> anyhow::Result<bool> {
        Ok(self.object_removed_kind(object_id)? == Some(RemovedObjectKind::Deleted))
    }

    /// Return whether local execution has wrapped the object.
    pub(crate) fn is_object_wrapped(&self, object_id: &ObjectID) -> anyhow::Result<bool> {
        Ok(self.object_removed_kind(object_id)? == Some(RemovedObjectKind::Wrapped))
    }

    /// Return whether local execution has made the object inaccessible by direct current ID
    /// lookup.
    pub(crate) fn is_object_currently_removed(&self, object_id: &ObjectID) -> anyhow::Result<bool> {
        Ok(self.object_removed_kind(object_id)?.is_some())
    }

    /// Write the current removal marker file for the given object reference.
    fn write_object_removed_marker(
        &self,
        kind: RemovedObjectKind,
        object_ref: &ObjectRef,
    ) -> anyhow::Result<()> {
        let object_id = object_ref.0;
        let object_dir = self.objects_dir().join(object_id.to_string());
        fs::create_dir_all(&object_dir)
            .with_context(|| format!("Failed to create directory: {}", object_dir.display()))?;
        let contents = format!(
            "{} {} {}\n",
            kind.marker_text(),
            object_ref.1.value(),
            object_ref.2
        );
        fs::write(object_dir.join(REMOVED_FILE), contents)
            .with_context(|| format!("Failed to mark object {} removed", object_id))
    }

    /// Clear the current removal marker if it matches `kind`.
    fn clear_object_removed_marker_kind(
        &self,
        object_id: &ObjectID,
        kind: RemovedObjectKind,
    ) -> anyhow::Result<()> {
        if self
            .read_object_removed_marker_kind(object_id)?
            .is_some_and(|removed_kind| removed_kind == kind)
        {
            self.clear_object_removed_marker(object_id)?;
        }
        Ok(())
    }

    /// Return the current removal kind.
    fn object_removed_kind(
        &self,
        object_id: &ObjectID,
    ) -> anyhow::Result<Option<RemovedObjectKind>> {
        self.read_object_removed_marker_kind(object_id)
    }

    /// Read the canonical `removed` marker.
    fn read_object_removed_marker_kind(
        &self,
        object_id: &ObjectID,
    ) -> anyhow::Result<Option<RemovedObjectKind>> {
        let path = self
            .objects_dir()
            .join(object_id.to_string())
            .join(REMOVED_FILE);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read object removed marker: {}", path.display()))?;
        let mut parts = content.split_whitespace();
        let kind = parts
            .next()
            .ok_or_else(|| anyhow!("Missing object removal kind in: {}", path.display()))
            .and_then(RemovedObjectKind::from_marker_text)?;
        let _version = parts
            .next()
            .ok_or_else(|| anyhow!("Missing object removal version in: {}", path.display()))?
            .parse::<u64>()
            .with_context(|| {
                format!(
                    "Failed to parse object removal version in: {}",
                    path.display()
                )
            })?;
        let _digest = parts
            .next()
            .ok_or_else(|| anyhow!("Missing object removal digest in: {}", path.display()))?
            .parse::<ObjectDigest>()
            .with_context(|| {
                format!(
                    "Failed to parse object removal digest in: {}",
                    path.display()
                )
            })?;
        anyhow::ensure!(
            parts.next().is_none(),
            "Unexpected extra object removal marker data in: {}",
            path.display()
        );

        Ok(Some(kind))
    }

    /// Clear the removal marker for the given object ID. If the file does not exist, this is a
    /// no-op.
    fn clear_object_removed_marker(&self, object_id: &ObjectID) -> anyhow::Result<()> {
        let path = self
            .objects_dir()
            .join(object_id.to_string())
            .join(REMOVED_FILE);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err)
                .with_context(|| format!("Failed to clear object marker: {}", path.display())),
        }
    }

    /// Read the owned-object index. Missing index files represent an empty index.
    pub(crate) fn get_owned_object_entries(&self) -> anyhow::Result<Vec<OwnedObjectEntry>> {
        let path = self.owned_objects_index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        self.read_bcs_file(&path)
    }

    /// Apply local execution ownership changes to the owned-object index.
    pub(crate) fn apply_owned_object_index_updates<'a>(
        &self,
        removed_object_ids: &[ObjectID],
        written_objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        let mut entries = self.get_owned_object_entries()?;

        for object_id in removed_object_ids {
            remove_owned_entry(&mut entries, *object_id);
        }

        for object in written_objects {
            match OwnedObjectEntry::from_object(object) {
                Some(entry) => upsert_owned_entry(&mut entries, entry),
                None => remove_owned_entry(&mut entries, object.id()),
            }
        }

        self.write_owned_object_entries(&entries)
    }

    /// Persist the owned-object index to disk. The entire index is rewritten on each update, but
    /// this is expected to be small and updated infrequently enough that this should not be a
    /// bottleneck.
    fn write_owned_object_entries(&self, entries: &[OwnedObjectEntry]) -> anyhow::Result<()> {
        let path = self.owned_objects_index_path();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let tmp_path = path.with_extension("tmp");
        let bytes = bcs::to_bytes(entries).with_context(|| {
            format!(
                "Failed to serialize owned-object index for: {}",
                path.display()
            )
        })?;
        fs::write(&tmp_path, bytes)
            .with_context(|| format!("Failed to write index file: {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("Failed to replace owned-object index: {}", path.display()))
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

    /// Path to the per-sequence directory holding `summary`.
    fn checkpoint_seq_dir(&self, sequence: CheckpointSequenceNumber) -> PathBuf {
        self.checkpoints_dir().join(sequence.to_string())
    }

    /// Path to the content-addressed contents directory.
    fn checkpoint_contents_dir(&self) -> PathBuf {
        self.checkpoints_dir().join(CHECKPOINT_CONTENTS_DIR)
    }

    /// Path to the file storing a specific `CheckpointContents` blob.
    fn checkpoint_contents_path(&self, digest: &CheckpointContentsDigest) -> PathBuf {
        self.checkpoint_contents_dir().join(digest.to_string())
    }

    fn checkpoint_digest_index_path(&self) -> PathBuf {
        self.checkpoints_dir().join(CHECKPOINT_DIGEST_INDEX_FILE)
    }

    /// Persist a checkpoint summary to `checkpoints/{seq}/summary`, append the
    /// checkpoint digest to the digest index, and bump the `latest` marker if
    /// `seq` is strictly higher than the currently recorded value.
    pub(crate) fn write_checkpoint_summary(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> anyhow::Result<()> {
        let sequence = checkpoint.data().sequence_number;
        let path = self
            .checkpoint_seq_dir(sequence)
            .join(CHECKPOINT_SUMMARY_FILE);
        self.write_bcs_file(&path, checkpoint.inner())?;

        self.update_latest_checkpoint_marker(sequence)?;
        append_index_line(
            &self.checkpoint_digest_index_path(),
            &checkpoint.digest().to_string(),
            sequence,
        )
    }

    /// Persist checkpoint contents to `checkpoints/contents/{digest}`.
    /// Contents are content-addressed, so this write is independent of the
    /// summary that references it.
    pub(crate) fn write_checkpoint_contents(
        &self,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        let path = self.checkpoint_contents_path(contents.digest());
        self.write_bcs_file(&path, contents)
    }

    /// Read a previously persisted checkpoint summary. Returns `None` if no
    /// summary file exists for the given sequence.
    pub(crate) fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let path = self
            .checkpoint_seq_dir(sequence)
            .join(CHECKPOINT_SUMMARY_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let certified: CertifiedCheckpointSummary = self.read_bcs_file(&path)?;
        Ok(Some(VerifiedCheckpoint::new_unchecked(certified)))
    }

    /// Read checkpoint contents for a given sequence by joining through the
    /// summary's `content_digest`. Returns `None` if either the summary or
    /// the referenced contents file is missing.
    pub(crate) fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        let Some(summary) = self.get_checkpoint_by_sequence_number(sequence)? else {
            return Ok(None);
        };
        self.get_checkpoint_contents_by_digest(&summary.data().content_digest)
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

    /// Resolve checkpoint contents by their digest via a direct filesystem
    /// lookup in the content-addressed contents directory.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        let path = self.checkpoint_contents_path(digest);
        if !path.exists() {
            return Ok(None);
        }
        self.read_bcs_file(&path).map(Some)
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
        if current.is_none_or(|c| sequence > c) {
            fs::write(&path, sequence.to_string()).with_context(|| {
                format!(
                    "Failed to write latest checkpoint marker: {}",
                    path.display()
                )
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

fn remove_owned_entry(entries: &mut Vec<OwnedObjectEntry>, object_id: ObjectID) {
    if let Ok(index) = entries.binary_search_by_key(&object_id, |entry| entry.object_id) {
        entries.remove(index);
    }
}

fn upsert_owned_entry(entries: &mut Vec<OwnedObjectEntry>, entry: OwnedObjectEntry) {
    match entries.binary_search_by_key(&entry.object_id, |existing| existing.object_id) {
        Ok(index) => entries[index] = entry,
        Err(index) => entries.insert(index, entry),
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
fn lookup_index(path: &Path, key: &str) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
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
#[path = "tests/filesystem.rs"]
mod tests;
