// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod casual_order;
mod checkpoint_output;
mod metrics;

use crate::authority::{AuthorityState, EffectsNotifyRead};
use crate::checkpoints::casual_order::CasualOrder;
use crate::checkpoints::checkpoint_output::{CertifiedCheckpointOutput, CheckpointOutput};
pub use crate::checkpoints::checkpoint_output::{
    LogCheckpointOutput, SendCheckpointToStateSync, SubmitCheckpointToConsensus,
};
pub use crate::checkpoints::metrics::CheckpointMetrics;
use crate::metrics::TaskUtilizationExt;
use fastcrypto::encoding::{Encoding, Hex};
use futures::future::{select, Either};
use futures::FutureExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_metrics::spawn_monitored_task;
use sui_types::base_types::{AuthorityName, TransactionDigest};
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::crypto::{AuthoritySignInfo, AuthorityWeakQuorumSignInfo};
use sui_types::error::{SuiError, SuiResult};
use sui_types::gas::GasCostSummary;
use sui_types::messages::TransactionEffects;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointContentsDigest, CheckpointDigest,
    CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointSummary, VerifiedCheckpoint,
};
use tokio::sync::{mpsc, watch, Notify};
use tracing::{debug, error, info, warn};
use typed_store::rocks::{DBMap, TypedStoreError};
use typed_store::traits::TypedStoreDebug;
use typed_store::Map;
use typed_store_derive::DBMapUtils;

type CheckpointCommitHeight = u64;

#[derive(DBMapUtils)]
pub struct CheckpointStore {
    /// This table has information for the checkpoints for which we constructed all the data
    /// from consensus, but not yet constructed actual checkpoint.
    ///
    /// Key in this table is the narwhal commit height and not a checkpoint sequence number.
    ///
    /// Non-empty list of transactions here might result in empty list when we are forming checkpoint.
    /// Because we don't want to create checkpoints with empty content(see CheckpointBuilder::write_checkpoint),
    /// the sequence number of checkpoint does not match height here.
    ///
    /// The boolean value indicates whether this is the last checkpoint of the epoch.
    pending_checkpoints: DBMap<CheckpointCommitHeight, (Vec<TransactionDigest>, bool)>,

    /// Maps checkpoint contents digest to checkpoint contents
    checkpoint_content: DBMap<CheckpointContentsDigest, CheckpointContents>,

    /// Maps sequence number to checkpoint summary
    checkpoint_summary: DBMap<CheckpointSequenceNumber, CheckpointSummary>,

    /// Lists all transaction digests included in checkpoints
    /// This can be cleaned up on epoch boundary
    digest_to_checkpoint: DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// Stores pending signatures
    /// The key in this table is checkpoint sequence number and an arbitrary integer
    /// This tables needs to be cleaned up on epoch boundary
    pending_signatures: DBMap<(CheckpointSequenceNumber, u64), CheckpointSignatureMessage>,

    /// Stores certified checkpoints
    certified_checkpoints: DBMap<CheckpointSequenceNumber, CertifiedCheckpointSummary>,
    /// Map from checkpoint digest to certified checkpoint
    checkpoint_by_digest: DBMap<CheckpointDigest, CertifiedCheckpointSummary>,

    /// Watermarks used to determine the highest verified and fully synced checkpoints
    watermarks: DBMap<CheckpointWatermark, (CheckpointSequenceNumber, CheckpointDigest)>,
}

impl CheckpointStore {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(Self::open_tables_read_write(path.to_path_buf(), None, None))
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.checkpoint_by_digest
            .get(digest)
            .map(|maybe_checkpoint| maybe_checkpoint.map(VerifiedCheckpoint::new_unchecked))
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.certified_checkpoints
            .get(&sequence_number)
            .map(|maybe_checkpoint| maybe_checkpoint.map(VerifiedCheckpoint::new_unchecked))
    }

    pub fn get_highest_verified_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_verified = if let Some(highest_verified) =
            self.watermarks.get(&CheckpointWatermark::HighestVerified)?
        {
            highest_verified
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_verified.1)
    }

    pub fn get_highest_synced_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_synced = if let Some(highest_synced) =
            self.watermarks.get(&CheckpointWatermark::HighestSynced)?
        {
            highest_synced
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_synced.1)
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, TypedStoreError> {
        self.checkpoint_content.get(digest)
    }

    pub fn insert_certified_checkpoint(
        &self,
        checkpoint: &CertifiedCheckpointSummary,
    ) -> Result<(), TypedStoreError> {
        self.certified_checkpoints
            .batch()
            .insert_batch(
                &self.certified_checkpoints,
                [(&checkpoint.sequence_number(), checkpoint)],
            )?
            .insert_batch(
                &self.checkpoint_by_digest,
                [(&checkpoint.digest(), checkpoint)],
            )?
            .write()
    }

    pub fn insert_verified_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.insert_certified_checkpoint(checkpoint.inner())?;

        // Update latest
        if Some(checkpoint.sequence_number())
            > self
                .get_highest_verified_checkpoint()?
                .map(|x| x.sequence_number())
        {
            self.watermarks.insert(
                &CheckpointWatermark::HighestVerified,
                &(checkpoint.sequence_number(), checkpoint.digest()),
            )?;
        }

        Ok(())
    }

    pub fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.watermarks.insert(
            &CheckpointWatermark::HighestSynced,
            &(checkpoint.sequence_number(), checkpoint.digest()),
        )
    }

    pub fn insert_checkpoint_contents(
        &self,
        contents: CheckpointContents,
    ) -> Result<(), TypedStoreError> {
        self.checkpoint_content
            .insert(&contents.digest(), &contents)
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointWatermark {
    HighestVerified,
    HighestSynced,
}

pub struct CheckpointBuilder {
    state: Arc<AuthorityState>,
    tables: Arc<CheckpointStore>,
    notify: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    effects_store: Box<dyn EffectsNotifyRead>,
    output: Box<dyn CheckpointOutput>,
    exit: watch::Receiver<()>,
    metrics: Arc<CheckpointMetrics>,
}

pub struct CheckpointAggregator {
    tables: Arc<CheckpointStore>,
    notify: Arc<Notify>,
    exit: watch::Receiver<()>,
    current: Option<SignatureAggregator>,
    state: Arc<AuthorityState>,
    output: Box<dyn CertifiedCheckpointOutput>,
    metrics: Arc<CheckpointMetrics>,
}

// This holds information to aggregate signatures for one checkpoint
pub struct SignatureAggregator {
    next_index: u64,
    summary: CheckpointSummary,
    digest: CheckpointDigest,
    signatures: HashMap<AuthorityName, AuthoritySignInfo>,
    stake: StakeUnit,
}

impl CheckpointBuilder {
    fn new(
        state: Arc<AuthorityState>,
        tables: Arc<CheckpointStore>,
        notify: Arc<Notify>,
        effects_store: Box<dyn EffectsNotifyRead>,
        output: Box<dyn CheckpointOutput>,
        exit: watch::Receiver<()>,
        notify_aggregator: Arc<Notify>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Self {
        Self {
            state,
            tables,
            notify,
            effects_store,
            output,
            exit,
            notify_aggregator,
            metrics,
        }
    }

    async fn run(mut self) {
        loop {
            for (height, (roots, last_checkpoint_of_epoch)) in
                self.tables.pending_checkpoints.iter()
            {
                if let Err(e) = self
                    .make_checkpoint(height, roots, last_checkpoint_of_epoch)
                    .await
                {
                    error!("Error while making checkpoint, will retry in 1s: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    self.metrics.checkpoint_errors.inc();
                    continue;
                }
            }
            match select(self.exit.changed().boxed(), self.notify.notified().boxed()).await {
                Either::Left(_) => {
                    // return on exit signal
                    return;
                }
                Either::Right(_) => {}
            }
        }
    }

    async fn make_checkpoint(
        &self,
        height: CheckpointCommitHeight,
        roots: Vec<TransactionDigest>,
        last_checkpoint_of_epoch: bool,
    ) -> SuiResult {
        let _timer = self.metrics.builder_utilization.utilization_timer();
        self.metrics
            .checkpoint_roots_count
            .inc_by(roots.len() as u64);
        let roots = self.effects_store.notify_read(roots).await?;
        let unsorted = self.complete_checkpoint_effects(roots)?;
        let sorted = CasualOrder::casual_sort(unsorted);
        let new_checkpoint = self
            .create_checkpoint(sorted, last_checkpoint_of_epoch)
            .await;
        self.write_checkpoint(height, new_checkpoint).await?;
        Ok(())
    }

    async fn write_checkpoint(
        &self,
        height: CheckpointCommitHeight,
        new_checkpoint: Option<(CheckpointSummary, CheckpointContents)>,
    ) -> SuiResult {
        let mut batch = self.tables.pending_checkpoints.batch();
        if let Some((summary, contents)) = new_checkpoint {
            // Only create checkpoint if content is not empty
            self.output.checkpoint_created(&summary, &contents).await?;

            self.metrics
                .transactions_included_in_checkpoint
                .inc_by(contents.size() as u64);
            let sequence_number = summary.sequence_number;
            self.metrics
                .last_constructed_checkpoint
                .set(sequence_number as i64);

            for txn in contents.iter() {
                batch = batch.insert_batch(
                    &self.tables.digest_to_checkpoint,
                    [(txn.transaction, sequence_number)],
                )?;
            }
            batch = batch.insert_batch(
                &self.tables.checkpoint_content,
                [(contents.digest(), contents)],
            )?;
            batch = batch.insert_batch(
                &self.tables.checkpoint_summary,
                [(sequence_number, summary)],
            )?;

            self.notify_aggregator.notify_waiters();
        }
        batch = batch.delete_batch(&self.tables.pending_checkpoints, [height])?;
        batch.write()?;
        Ok(())
    }

    async fn create_checkpoint(
        &self,
        mut effects: Vec<TransactionEffects>,
        last_checkpoint_of_epoch: bool,
    ) -> Option<(CheckpointSummary, CheckpointContents)> {
        let last_checkpoint = self.tables.checkpoint_summary.iter().skip_to_last().next();
        let epoch_rolling_gas_cost_summary = Self::get_epoch_total_gas_cost(
            last_checkpoint.as_ref().map(|(_, c)| c),
            &effects,
            self.state.epoch(),
        );
        if last_checkpoint_of_epoch {
            if let Err(err) = self
                .augment_epoch_last_checkpoint(&epoch_rolling_gas_cost_summary, &mut effects)
                .await
            {
                error!(
                    "Failed to augment the last checkpoint of the epoch: {:?}",
                    err
                );
                // TODO: Is returning None the best we can do here?
                return None;
            }
        }
        if effects.is_empty() {
            return None;
        }
        let contents = CheckpointContents::new_with_causally_ordered_transactions(
            effects.iter().map(TransactionEffects::execution_digests),
        );
        let previous_digest = last_checkpoint.as_ref().map(|(_, c)| c.digest());
        let sequence_number = last_checkpoint
            .as_ref()
            .map(|(_, c)| c.sequence_number + 1)
            .unwrap_or_default();
        let summary = CheckpointSummary::new(
            self.state.epoch(),
            sequence_number,
            &contents,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            if last_checkpoint_of_epoch {
                Some(
                    self.state
                        .get_sui_system_state_object()
                        .await
                        .unwrap()
                        .get_current_epoch_committee()
                        .committee,
                )
            } else {
                None
            },
        );
        Some((summary, contents))
    }

    fn get_epoch_total_gas_cost(
        last_checkpoint: Option<&CheckpointSummary>,
        cur_checkpoint_effects: &[TransactionEffects],
        cur_epoch: EpochId,
    ) -> GasCostSummary {
        let (previous_epoch, previous_gas_costs) = last_checkpoint
            .map(|c| (c.epoch, c.epoch_rolling_gas_cost_summary.clone()))
            .unwrap_or_default();
        let current_gas_costs = GasCostSummary::new_from_txn_effects(cur_checkpoint_effects.iter());
        if previous_epoch == cur_epoch {
            // sum only when we are within the same epoch
            GasCostSummary::new(
                previous_gas_costs.computation_cost + current_gas_costs.computation_cost,
                previous_gas_costs.storage_cost + current_gas_costs.storage_cost,
                previous_gas_costs.storage_rebate + current_gas_costs.storage_rebate,
            )
        } else {
            current_gas_costs
        }
    }

    async fn augment_epoch_last_checkpoint(
        &self,
        epoch_total_gas_cost: &GasCostSummary,
        effects: &mut Vec<TransactionEffects>,
    ) -> anyhow::Result<()> {
        let cert = self
            .state
            .create_advance_epoch_tx_cert(
                self.state.epoch() + 1,
                epoch_total_gas_cost,
                Duration::from_secs(60), // TODO: Is 60s enough?
            )
            .await?;
        let signed_effect = self
            .state
            .handle_certificate(&cert)
            .await?
            .signed_effects
            .unwrap();
        effects.push(signed_effect.into_data());
        Ok(())
    }

    /// For the given roots return complete list of effects to include in checkpoint
    /// This list includes the roots and all their dependencies, which are not part of checkpoint already
    fn complete_checkpoint_effects(
        &self,
        mut roots: Vec<TransactionEffects>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let mut results = vec![];
        let mut seen = HashSet::new();
        loop {
            let mut pending = HashSet::new();
            for effect in roots {
                let digest = effect.transaction_digest;
                if self.tables.digest_to_checkpoint.contains_key(&digest)? {
                    continue;
                }
                for dependency in effect.dependencies.iter() {
                    if seen.insert(*dependency) {
                        pending.insert(*dependency);
                    }
                }
                results.push(effect);
            }
            if pending.is_empty() {
                break;
            }
            let pending = pending.into_iter().collect::<Vec<_>>();
            let effects = self.effects_store.get_effects(&pending)?;
            let effects = effects
                .into_iter()
                .zip(pending.into_iter())
                .map(|(opt, digest)| match opt {
                    Some(x) => x,
                    None => panic!(
                        "Can not find effect for transaction {:?}, however transaction that depend on it was already executed",
                        digest
                    ),
                })
                .collect::<Vec<_>>();
            roots = effects;
        }
        Ok(results)
    }
}

impl CheckpointAggregator {
    fn new(
        tables: Arc<CheckpointStore>,
        notify: Arc<Notify>,
        exit: watch::Receiver<()>,
        state: Arc<AuthorityState>,
        output: Box<dyn CertifiedCheckpointOutput>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Self {
        let current = None;
        Self {
            tables,
            notify,
            exit,
            current,
            state,
            output,
            metrics,
        }
    }

    async fn run(mut self) {
        loop {
            if let Err(e) = self.run_inner().await {
                error!(
                    "Error while aggregating checkpoint, will retry in 1s: {:?}",
                    e
                );
                self.metrics.checkpoint_errors.inc();
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            match select(self.exit.changed().boxed(), self.notify.notified().boxed()).await {
                Either::Left(_) => {
                    // return on exit signal
                    return;
                }
                Either::Right(_) => {}
            }
        }
    }

    async fn run_inner(&mut self) -> SuiResult {
        let _timer = self.metrics.aggregator_utilization.utilization_timer();
        'outer: loop {
            let current = if let Some(current) = &mut self.current {
                current
            } else {
                let next_to_certify = self.next_checkpoint_to_certify();
                let Some(summary) = self.tables.checkpoint_summary.get(&next_to_certify)? else { return Ok(()); };
                self.current = Some(SignatureAggregator {
                    next_index: 0,
                    digest: summary.digest(),
                    summary,
                    signatures: Default::default(),
                    stake: Default::default(),
                });
                self.current.as_mut().unwrap()
            };
            let key = (current.summary.sequence_number, current.next_index);
            let iter = self.tables.pending_signatures.iter().skip_to(&key)?;
            debug!("Scanning pending checkpoint signatures from {:?}", key);
            for ((seq, index), data) in iter {
                if seq != current.summary.sequence_number {
                    debug!(
                        "Not enough checkpoint signatures on height {}",
                        current.summary.sequence_number
                    );
                    // No more signatures (yet) for this checkpoint
                    return Ok(());
                }
                debug!(
                    "Processing signature for checkpoint {} from {:?}",
                    current.summary.sequence_number,
                    data.summary.auth_signature.authority.concise()
                );
                self.metrics
                    .checkpoint_participation
                    .with_label_values(&[&format!(
                        "{:?}",
                        data.summary.auth_signature.authority.concise()
                    )])
                    .inc();
                if let Ok(auth_signature) = current.try_aggregate(&self.state.committee(), data) {
                    let summary = CertifiedCheckpointSummary {
                        summary: current.summary.clone(),
                        auth_signature,
                    };
                    self.tables.insert_certified_checkpoint(&summary)?;
                    self.metrics
                        .last_certified_checkpoint
                        .set(current.summary.sequence_number as i64);
                    self.output.certified_checkpoint_created(&summary).await?;
                    self.current = None;
                    continue 'outer;
                } else {
                    current.next_index = index + 1;
                }
            }
            break;
        }
        Ok(())
    }

    fn next_checkpoint_to_certify(&self) -> CheckpointSequenceNumber {
        self.tables
            .certified_checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq + 1)
            .unwrap_or_default()
    }
}

impl SignatureAggregator {
    #[allow(clippy::result_unit_err)]
    pub fn try_aggregate(
        &mut self,
        committee: &Committee,
        data: CheckpointSignatureMessage,
    ) -> Result<AuthorityWeakQuorumSignInfo, ()> {
        let their_digest = data.summary.summary.digest();
        let author = data.summary.auth_signature.authority;
        let signature = data.summary.auth_signature;
        if their_digest != self.digest {
            // todo - consensus need to ensure data.summary.auth_signature.authority == narwhal_cert.author
            warn!(
                "Validator {:?} has mismatching checkpoint digest {} at seq {}, we have digest {}",
                author.concise(),
                Hex::encode(their_digest),
                self.summary.sequence_number,
                Hex::encode(self.digest)
            );
            return Err(());
        }
        match self.signatures.entry(author) {
            Entry::Occupied(oc) => {
                if oc.get() != &signature {
                    warn!("Validator {:?} submitted two different signatures for checkpoint {}: {:?}, {:?}", author.concise(), self.summary.sequence_number, oc.get(), signature);
                    return Err(());
                }
            }
            Entry::Vacant(va) => {
                va.insert(signature);
            }
        }
        self.stake += committee.weight(&author);
        if self.stake >= committee.validity_threshold() {
            let signatures = self.signatures.values().cloned().collect();
            match AuthorityWeakQuorumSignInfo::new_from_auth_sign_infos(signatures, committee) {
                Ok(aggregated) => Ok(aggregated),
                Err(err) => {
                    error!(
                        "Unexpected error when aggregating signatures for checkpoint {}: {:?}",
                        self.summary.sequence_number, err
                    );
                    Err(())
                }
            }
        } else {
            Err(())
        }
    }
}

pub trait CheckpointServiceNotify {
    fn notify_checkpoint_signature(&self, info: &CheckpointSignatureMessage) -> SuiResult;

    fn notify_checkpoint(
        &self,
        index: CheckpointCommitHeight,
        roots: Vec<TransactionDigest>,
        last_checkpoint_of_epoch: bool,
    ) -> SuiResult;
}

/// This is a service used to communicate with other pieces of sui(for ex. authority)
pub struct CheckpointService {
    tables: Arc<CheckpointStore>,
    notify_builder: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    last_signature_index: Mutex<u64>,
    _exit: watch::Sender<()>, // dropping this will eventually stop checkpoint tasks
}

impl CheckpointService {
    pub fn spawn(
        state: Arc<AuthorityState>,
        checkpoint_store: Arc<CheckpointStore>,
        effects_store: Box<dyn EffectsNotifyRead>,
        checkpoint_output: Box<dyn CheckpointOutput>,
        certified_checkpoint_output: Box<dyn CertifiedCheckpointOutput>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Arc<Self> {
        let notify_builder = Arc::new(Notify::new());
        let notify_aggregator = Arc::new(Notify::new());

        let (exit_snd, exit_rcv) = watch::channel(());

        let builder = CheckpointBuilder::new(
            state.clone(),
            checkpoint_store.clone(),
            notify_builder.clone(),
            effects_store,
            checkpoint_output,
            exit_rcv.clone(),
            notify_aggregator.clone(),
            metrics.clone(),
        );

        spawn_monitored_task!(builder.run());

        let aggregator = CheckpointAggregator::new(
            checkpoint_store.clone(),
            notify_aggregator.clone(),
            exit_rcv,
            state,
            certified_checkpoint_output,
            metrics,
        );

        spawn_monitored_task!(aggregator.run());

        let last_signature_index = checkpoint_store
            .pending_signatures
            .iter()
            .skip_to_last()
            .next()
            .map(|((_, index), _)| index)
            .unwrap_or_default();

        let last_signature_index = Mutex::new(last_signature_index);

        Arc::new(Self {
            tables: checkpoint_store,
            notify_builder,
            notify_aggregator,
            last_signature_index,
            _exit: exit_snd,
        })
    }

    /// Used by internal systems that want to subscribe to checkpoints.
    /// Returned sender will contain all checkpoints starting from(inclusive) given sequence number
    /// CheckpointSequenceNumber::default() can be used to start from the beginning
    pub fn subscribe_checkpoints(
        &self,
        from_sequence: CheckpointSequenceNumber,
    ) -> mpsc::Receiver<(CheckpointSummary, CheckpointContents)> {
        let (sender, receiver) = mpsc::channel(8);
        let tailer = CheckpointTailer {
            sender,
            sequence: from_sequence,
            tables: self.tables.clone(),
            notify: self.notify_aggregator.clone(),
        };
        spawn_monitored_task!(tailer.run());
        receiver
    }
}

impl CheckpointServiceNotify for CheckpointService {
    fn notify_checkpoint_signature(&self, info: &CheckpointSignatureMessage) -> SuiResult {
        let sequence = info.summary.summary.sequence_number;
        if let Some((last_certified, _)) = self
            .tables
            .certified_checkpoints
            .iter()
            .skip_to_last()
            .next()
        {
            if sequence <= last_certified {
                debug!(
                    "Ignore signature for checkpoint sequence {} from {} - already certified",
                    info.summary.summary.sequence_number, info.summary.auth_signature.authority
                );
                return Ok(());
            }
        }
        info!(
            "Received signature for checkpoint sequence {}, digest {} from {}",
            sequence,
            Hex::encode(info.summary.summary.digest()),
            info.summary.auth_signature.authority
        );
        // While it can be tempting to make last_signature_index into AtomicU64, this won't work
        // We need to make sure we write to `pending_signatures` and trigger `notify_aggregator` without race conditions
        let mut index = self.last_signature_index.lock();
        *index += 1;
        let key = (sequence, *index);
        self.tables.pending_signatures.insert(&key, info)?;
        self.notify_aggregator.notify_one();
        Ok(())
    }

    fn notify_checkpoint(
        &self,
        index: CheckpointCommitHeight,
        roots: Vec<TransactionDigest>,
        last_checkpoint_of_epoch: bool,
    ) -> SuiResult {
        if let Some(pending) = self.tables.pending_checkpoints.get(&index)? {
            if pending.0 != roots {
                panic!("Received checkpoint at index {} that contradicts previously stored checkpoint. Old digests: {:?}, new digests: {:?}", index, pending, roots);
            }
            debug!(
                "Ignoring duplicate checkpoint notification at height {}",
                index
            );
            return Ok(());
        }
        debug!(
            "Transaction roots for pending checkpoint {}: {:?}",
            index, roots
        );
        self.tables
            .pending_checkpoints
            .insert(&index, &(roots, last_checkpoint_of_epoch))?;
        self.notify_builder.notify_one();
        Ok(())
    }
}

#[cfg(test)]
pub struct CheckpointServiceNoop {}
#[cfg(test)]
impl CheckpointServiceNotify for CheckpointServiceNoop {
    fn notify_checkpoint_signature(&self, _: &CheckpointSignatureMessage) -> SuiResult {
        Ok(())
    }

    fn notify_checkpoint(
        &self,
        _: CheckpointCommitHeight,
        _: Vec<TransactionDigest>,
        _: bool,
    ) -> SuiResult {
        Ok(())
    }
}

struct CheckpointTailer {
    sequence: CheckpointSequenceNumber,
    sender: mpsc::Sender<(CheckpointSummary, CheckpointContents)>,
    tables: Arc<CheckpointStore>,
    notify: Arc<Notify>,
}

impl CheckpointTailer {
    async fn run(mut self) {
        loop {
            match self.do_run().await {
                Err(err) => {
                    error!(
                        "Error while tailing checkpoint, will retry in 1s: {:?}",
                        err
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Ok(true) => {}
                Ok(false) => return,
            }
            self.notify.notified().await;
        }
    }

    // Returns Ok(false) if sender channel is closed
    async fn do_run(&mut self) -> SuiResult<bool> {
        loop {
            let summary = self.tables.checkpoint_summary.get(&self.sequence)?;
            let Some(summary) = summary else { return Ok(true); };
            let content = self
                .tables
                .checkpoint_content
                .get(&summary.content_digest)?;
            let Some(content) = content else {
                return Err(SuiError::from("Checkpoint summary for sequence {} exists, but content does not. This should not happen"));
            };
            if self.sender.send((summary, content)).await.is_err() {
                return Ok(false);
            }
            self.sequence += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use fastcrypto::traits::KeyPair;
    use std::collections::HashMap;
    use sui_types::crypto::AuthorityKeyPair;
    use sui_types::messages_checkpoint::SignedCheckpointSummary;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    #[tokio::test]
    pub async fn checkpoint_builder_test() {
        let tempdir = tempdir().unwrap();
        let mut store: HashMap<TransactionDigest, TransactionEffects> = HashMap::new();
        store.insert(
            d(1),
            e(d(1), vec![d(2), d(3)], GasCostSummary::new(11, 12, 13)),
        );
        store.insert(
            d(2),
            e(d(2), vec![d(3), d(4)], GasCostSummary::new(21, 22, 23)),
        );
        store.insert(d(3), e(d(3), vec![], GasCostSummary::new(31, 32, 33)));
        store.insert(d(4), e(d(4), vec![], GasCostSummary::new(41, 42, 43)));
        let (output, mut result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
        let (certified_output, mut certified_result) =
            mpsc::channel::<CertifiedCheckpointSummary>(10);
        let store = Box::new(store);

        let (keypair, committee) = committee();
        let (tx_reconfigure_consensus, _rx_reconfigure_consensus) = mpsc::channel(10);
        let state = Arc::new(
            AuthorityState::new_for_testing(
                committee.clone(),
                &keypair,
                None,
                None,
                tx_reconfigure_consensus,
            )
            .await,
        );

        let checkpoint_store = CheckpointStore::new(tempdir.path());
        let checkpoint_service = CheckpointService::spawn(
            state,
            checkpoint_store,
            store,
            Box::new(output),
            Box::new(certified_output),
            CheckpointMetrics::new_for_tests(),
        );
        let mut tailer = checkpoint_service.subscribe_checkpoints(0);
        checkpoint_service
            .notify_checkpoint(0, vec![d(4)], false)
            .unwrap();
        // Verify that sending same digests at same height is noop
        checkpoint_service
            .notify_checkpoint(0, vec![d(4)], false)
            .unwrap();
        checkpoint_service
            .notify_checkpoint(1, vec![d(1), d(3)], false)
            .unwrap();

        let (c1c, c1s) = result.recv().await.unwrap();
        let (c2c, c2s) = result.recv().await.unwrap();

        let c1t = c1c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        let c2t = c2c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c1t, vec![d(4)]);
        assert_eq!(c1s.previous_digest, None);
        assert_eq!(c1s.sequence_number, 0);
        assert_eq!(
            c1s.epoch_rolling_gas_cost_summary,
            GasCostSummary::new(41, 42, 43)
        );

        assert_eq!(c2t, vec![d(3), d(2), d(1)]);
        assert_eq!(c2s.previous_digest, Some(c1s.digest()));
        assert_eq!(c2s.sequence_number, 1);
        assert_eq!(
            c2s.epoch_rolling_gas_cost_summary,
            GasCostSummary::new(104, 108, 112)
        );

        let c1ss =
            SignedCheckpointSummary::new_from_summary(c1s, keypair.public().into(), &keypair);
        let c2ss =
            SignedCheckpointSummary::new_from_summary(c2s, keypair.public().into(), &keypair);

        checkpoint_service
            .notify_checkpoint_signature(&CheckpointSignatureMessage { summary: c2ss })
            .unwrap();
        checkpoint_service
            .notify_checkpoint_signature(&CheckpointSignatureMessage { summary: c1ss })
            .unwrap();

        let c1sc = certified_result.recv().await.unwrap();
        let c2sc = certified_result.recv().await.unwrap();
        assert_eq!(c1sc.summary.sequence_number, 0);
        assert_eq!(c2sc.summary.sequence_number, 1);

        let (t1s, _content) = tailer.recv().await.unwrap();
        let (t2s, _content) = tailer.recv().await.unwrap();
        assert_eq!(t1s.sequence_number, 0);
        assert_eq!(t2s.sequence_number, 1);
    }

    #[async_trait]
    impl EffectsNotifyRead for HashMap<TransactionDigest, TransactionEffects> {
        async fn notify_read(
            &self,
            digests: Vec<TransactionDigest>,
        ) -> SuiResult<Vec<TransactionEffects>> {
            Ok(digests
                .into_iter()
                .map(|d| self.get(d.as_ref()).expect("effects not found").clone())
                .collect())
        }

        fn get_effects(
            &self,
            digests: &[TransactionDigest],
        ) -> SuiResult<Vec<Option<TransactionEffects>>> {
            Ok(digests
                .iter()
                .map(|d| self.get(d.as_ref()).cloned())
                .collect())
        }
    }

    #[async_trait::async_trait]
    impl CheckpointOutput for mpsc::Sender<(CheckpointContents, CheckpointSummary)> {
        async fn checkpoint_created(
            &self,
            summary: &CheckpointSummary,
            contents: &CheckpointContents,
        ) -> SuiResult {
            self.try_send((contents.clone(), summary.clone())).unwrap();
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl CertifiedCheckpointOutput for mpsc::Sender<CertifiedCheckpointSummary> {
        async fn certified_checkpoint_created(
            &self,
            summary: &CertifiedCheckpointSummary,
        ) -> SuiResult {
            self.try_send(summary.clone()).unwrap();
            Ok(())
        }
    }

    fn d(i: u8) -> TransactionDigest {
        let mut bytes: [u8; 32] = Default::default();
        bytes[0] = i;
        TransactionDigest::new(bytes)
    }

    fn e(
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
        gas_used: GasCostSummary,
    ) -> TransactionEffects {
        TransactionEffects {
            transaction_digest,
            dependencies,
            gas_used,
            ..Default::default()
        }
    }

    fn committee() -> (AuthorityKeyPair, Committee) {
        use std::collections::BTreeMap;
        use sui_types::crypto::get_key_pair;
        use sui_types::crypto::AuthorityPublicKeyBytes;

        let (_authority_address, authority_key): (_, AuthorityKeyPair) = get_key_pair();
        let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
        authorities.insert(
            /* address */ authority_key.public().into(),
            /* voting right */ 1,
        );
        (authority_key, Committee::new(0, authorities).unwrap())
    }
}
