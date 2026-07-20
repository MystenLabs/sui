// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-memory staging for the checkpoint currently being sealed.
//!
//! Simulacrum hands the store a checkpoint summary, then its transactions,
//! effects, and events piecemeal through the `SimulatorStore` insert methods,
//! and finally the checkpoint contents. This buffer holds those pieces until
//! the contents arrive, so the caller can persist the whole checkpoint into
//! the rpc-store as one consistent unit. Reads never consult this buffer:
//! sealing completes synchronously inside the checkpoint publication path, so
//! by the time an execution returns, its rows are already in the rpc-store.
//!
//! Nothing here is persisted: staged entries that have not been sealed are
//! lost on process restart.

use std::collections::BTreeMap;
use std::sync::RwLock;

use anyhow::anyhow;
use anyhow::bail;

use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::transaction::VerifiedTransaction;

#[derive(Default)]
struct PendingTransaction {
    transaction: Option<VerifiedTransaction>,
    effects: Option<TransactionEffects>,
    events: Option<TransactionEvents>,
}

/// One fully staged transaction, cloned out of the buffer at seal time.
pub(crate) struct StagedTransaction {
    pub(crate) digest: TransactionDigest,
    pub(crate) transaction: VerifiedTransaction,
    pub(crate) effects: TransactionEffects,
    pub(crate) events: TransactionEvents,
}

/// Staging buffer for the in-flight checkpoint and its transactions.
///
/// `RwLock` provides the interior mutability needed behind the shared
/// `Arc<DataStoreInner>` that every cloned `DataStore` holds.
#[derive(Default)]
pub(crate) struct PendingCheckpointBuffer {
    checkpoint: RwLock<Option<VerifiedCheckpoint>>,
    transactions: RwLock<BTreeMap<TransactionDigest, PendingTransaction>>,
}

impl PendingCheckpointBuffer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> anyhow::Result<()> {
        let mut pending = self
            .checkpoint
            .write()
            .map_err(|_| anyhow!("pending checkpoint lock poisoned"))?;
        *pending = Some(checkpoint);
        Ok(())
    }

    pub(crate) fn record_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> anyhow::Result<()> {
        let digest = *transaction.digest();
        let mut pending = self
            .transactions
            .write()
            .map_err(|_| anyhow!("pending transaction lock poisoned"))?;
        pending.entry(digest).or_default().transaction = Some(transaction);
        Ok(())
    }

    pub(crate) fn record_effects(&self, effects: TransactionEffects) -> anyhow::Result<()> {
        let digest = *effects.transaction_digest();
        let mut pending = self
            .transactions
            .write()
            .map_err(|_| anyhow!("pending transaction lock poisoned"))?;
        pending.entry(digest).or_default().effects = Some(effects);
        Ok(())
    }

    pub(crate) fn record_events(
        &self,
        tx_digest: TransactionDigest,
        events: TransactionEvents,
    ) -> anyhow::Result<()> {
        let mut pending = self
            .transactions
            .write()
            .map_err(|_| anyhow!("pending transaction lock poisoned"))?;
        pending.entry(tx_digest).or_default().events = Some(events);
        Ok(())
    }

    /// Return the staged checkpoint matching `contents`, validating that the
    /// contents digest is the one the checkpoint committed to.
    pub(crate) fn checkpoint_for_contents(
        &self,
        contents: &CheckpointContents,
    ) -> anyhow::Result<VerifiedCheckpoint> {
        let pending = self
            .checkpoint
            .read()
            .map_err(|_| anyhow!("pending checkpoint lock poisoned"))?;
        let Some(checkpoint) = pending.as_ref() else {
            bail!(
                "checkpoint contents {} inserted without a pending checkpoint",
                contents.digest(),
            );
        };
        if checkpoint.data().content_digest != *contents.digest() {
            bail!(
                "pending checkpoint {} references contents {}, but inserted contents are {}",
                checkpoint.data().sequence_number,
                checkpoint.data().content_digest,
                contents.digest(),
            );
        }
        Ok(checkpoint.clone())
    }

    /// Clone out every staged transaction referenced by `contents`, in
    /// checkpoint order. Entries stay staged until [`Self::clear_sealed`]
    /// confirms they were durably saved.
    pub(crate) fn staged_transactions_for(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<Vec<StagedTransaction>> {
        let pending = self
            .transactions
            .read()
            .map_err(|_| anyhow!("pending transaction lock poisoned"))?;
        let mut staged = Vec::new();
        for execution in contents.iter() {
            let digest = execution.transaction;
            let Some(entry) = pending.get(&digest) else {
                bail!(
                    "checkpoint {} references transaction {digest}, but no pending transaction was recorded",
                    checkpoint.data().sequence_number,
                );
            };
            let transaction = entry.transaction.clone().ok_or_else(|| {
                anyhow!(
                    "checkpoint {} references transaction {digest}, but transaction data is missing",
                    checkpoint.data().sequence_number,
                )
            })?;
            let effects = entry.effects.clone().ok_or_else(|| {
                anyhow!(
                    "checkpoint {} references transaction {digest}, but transaction effects are missing",
                    checkpoint.data().sequence_number,
                )
            })?;
            let events = entry.events.clone().unwrap_or_default();
            staged.push(StagedTransaction {
                digest,
                transaction,
                effects,
                events,
            });
        }
        Ok(staged)
    }

    /// Drop the staged entries for a checkpoint whose rows were durably saved.
    pub(crate) fn clear_sealed(
        &self,
        checkpoint: &VerifiedCheckpoint,
        digests: impl IntoIterator<Item = TransactionDigest>,
    ) -> anyhow::Result<()> {
        let mut pending = self
            .transactions
            .write()
            .map_err(|_| anyhow!("pending transaction lock poisoned"))?;
        for digest in digests {
            pending.remove(&digest);
        }
        drop(pending);

        let mut pending_checkpoint = self
            .checkpoint
            .write()
            .map_err(|_| anyhow!("pending checkpoint lock poisoned"))?;
        if pending_checkpoint
            .as_ref()
            .is_some_and(|pending| pending.digest() == checkpoint.digest())
        {
            *pending_checkpoint = None;
        }
        Ok(())
    }
}
