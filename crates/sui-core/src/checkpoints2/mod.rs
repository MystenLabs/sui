// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod casual_order;
mod checkpoint_output;

use crate::authority::EffectsNotifyRead;
use crate::checkpoints2::casual_order::CasualOrder;
use crate::checkpoints2::checkpoint_output::CheckpointOutput;
pub use crate::checkpoints2::checkpoint_output::LogCheckpointOutput;
use futures::future::{select, Either};
use futures::FutureExt;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_metrics::spawn_monitored_task;
use sui_types::base_types::TransactionDigest;
use sui_types::error::SuiResult;
use sui_types::gas::GasCostSummary;
use sui_types::messages::TransactionEffects;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
};
use tokio::sync::{oneshot, Notify};
use tracing::{debug, error};
use typed_store::rocks::{DBBatch, DBMap};
use typed_store::traits::TypedStoreDebug;
use typed_store::Map;
use typed_store_derive::DBMapUtils;

type CheckpointCommitHeight = u64;

#[derive(DBMapUtils)]
struct CheckpointStoreTables {
    /// This table has information for the checkpoints for which we constructed all the data
    /// from consensus, but not yet constructed actual checkpoint.
    ///
    /// Key in this table is the narwhal commit height and not a checkpoint sequence number.
    ///
    /// Non-empty list of transactions here might result in empty list when we are forming checkpoint.
    /// Because we don't want to create checkpoints with empty content(see CheckpointBuilder::write_checkpoint),
    /// the sequence number of checkpoint does not match height here.
    pending_checkpoints: DBMap<CheckpointCommitHeight, Vec<TransactionDigest>>,

    /// Maps sequence number to checkpoint contents
    checkpoint_content: DBMap<CheckpointSequenceNumber, CheckpointContents>,

    /// Maps sequence number to checkpoint summary
    checkpoint_summary: DBMap<CheckpointSequenceNumber, CheckpointSummary>,

    /// Lists all transaction digests included in checkpoints
    /// This can be cleaned up on epoch boundary
    digest_to_checkpoint: DBMap<TransactionDigest, CheckpointSequenceNumber>,
}

pub struct CheckpointBuilder {
    tables: Arc<CheckpointStoreTables>,
    notify: Arc<Notify>,
    effects_store: Box<dyn EffectsNotifyRead>,
    output: Box<dyn CheckpointOutput>,
    exit: oneshot::Receiver<()>,
}

impl CheckpointBuilder {
    fn new(
        tables: Arc<CheckpointStoreTables>,
        notify: Arc<Notify>,
        effects_store: Box<dyn EffectsNotifyRead>,
        output: Box<dyn CheckpointOutput>,
        exit: oneshot::Receiver<()>,
    ) -> Self {
        Self {
            tables,
            notify,
            effects_store,
            output,
            exit,
        }
    }

    async fn run(mut self) {
        loop {
            for (height, roots) in self.tables.pending_checkpoints.iter() {
                if let Err(e) = self.make_checkpoint(height, roots).await {
                    error!("Error while making checkpoint, will retry in 1s: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
            match select(&mut self.exit, self.notify.notified().boxed()).await {
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
    ) -> SuiResult {
        let roots = self.effects_store.notify_read(roots).await?;
        let unsorted = self.complete_checkpoint(roots)?;
        let sorted = CasualOrder::casual_sort(unsorted);
        self.write_checkpoint(height, sorted)?;
        Ok(())
    }

    fn write_checkpoint(
        &self,
        height: CheckpointCommitHeight,
        l: Vec<TransactionEffects>,
    ) -> SuiResult {
        let mut batch = self.tables.pending_checkpoints.batch();
        if !l.is_empty() {
            // Only create checkpoint if content is not empty
            batch = self.create_checkpoint(batch, l)?;
        }
        batch = batch.delete_batch(&self.tables.pending_checkpoints, [height])?;
        batch.write()?;
        Ok(())
    }

    fn create_checkpoint(
        &self,
        mut batch: DBBatch,
        l: Vec<TransactionEffects>,
    ) -> SuiResult<DBBatch> {
        let last_checkpoint = self.tables.checkpoint_summary.iter().skip_to_last().next();
        let previous_digest = last_checkpoint.as_ref().map(|(_, c)| c.digest());
        let sequence_number = last_checkpoint
            .map(|(_, c)| c.sequence_number + 1)
            .unwrap_or_default();
        let contents = CheckpointContents::new_with_causally_ordered_transactions(
            l.iter().map(TransactionEffects::execution_digests),
        );
        let gas_cost_summary = GasCostSummary::new_from_txn_effects(l.iter());
        let summary = CheckpointSummary::new(
            0, //todo
            sequence_number,
            &contents,
            previous_digest,
            gas_cost_summary,
            None, //todo
        );

        self.output.checkpoint_created(&summary, &contents)?;

        batch = batch.insert_batch(
            &self.tables.checkpoint_content,
            [(sequence_number, contents)],
        )?;
        batch = batch.insert_batch(
            &self.tables.checkpoint_summary,
            [(sequence_number, summary)],
        )?;
        for txn in l.iter() {
            batch = batch.insert_batch(
                &self.tables.digest_to_checkpoint,
                [(txn.transaction_digest, sequence_number)],
            )?;
        }
        Ok(batch)
    }

    /// For the given roots return complete list of effects to include in checkpoint
    /// This list includes the roots and all their dependencies, which are not part of checkpoint already
    fn complete_checkpoint(
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

/// This is a service used to communicate with other pieces of sui(for ex. authority)
pub struct CheckpointService {
    tables: Arc<CheckpointStoreTables>,
    notify: Arc<Notify>,
    _exit: oneshot::Sender<()>, // dropping this will eventually stop checkpoint tasks
}

impl CheckpointService {
    #[allow(dead_code)]
    pub fn spawn(
        path: &Path,
        effects_store: Box<dyn EffectsNotifyRead>,
        output: Box<dyn CheckpointOutput>,
    ) -> Arc<Self> {
        let notify = Arc::new(Notify::new());

        let tables = CheckpointStoreTables::open_tables_read_write(path.to_path_buf(), None, None);
        let tables = Arc::new(tables);

        let (exit_snd, exit_rcv) = oneshot::channel();

        let builder = CheckpointBuilder::new(
            tables.clone(),
            notify.clone(),
            effects_store,
            output,
            exit_rcv,
        );

        spawn_monitored_task!(builder.run());

        Arc::new(Self {
            tables,
            notify,
            _exit: exit_snd,
        })
    }

    pub fn notify_checkpoint(
        &self,
        index: CheckpointCommitHeight,
        roots: Vec<TransactionDigest>,
    ) -> SuiResult {
        if let Some(pending) = self.tables.pending_checkpoints.get(&index)? {
            if pending != roots {
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
        self.tables.pending_checkpoints.insert(&index, &roots)?;
        self.notify.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    #[tokio::test]
    pub async fn checkpoint_builder_test() {
        let tempdir = tempdir().unwrap();
        let mut store: HashMap<TransactionDigest, TransactionEffects> = HashMap::new();
        store.insert(d(1), e(d(1), vec![d(2), d(3)]));
        store.insert(d(2), e(d(2), vec![d(3), d(4)]));
        store.insert(d(3), e(d(3), vec![]));
        store.insert(d(4), e(d(4), vec![]));
        let (output, mut result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
        let store = Box::new(store);

        let checkpoint_service = CheckpointService::spawn(tempdir.path(), store, Box::new(output));
        checkpoint_service.notify_checkpoint(0, vec![d(4)]).unwrap();
        // Verify that sending same digests at same height is noop
        checkpoint_service.notify_checkpoint(0, vec![d(4)]).unwrap();
        checkpoint_service
            .notify_checkpoint(1, vec![d(1), d(3)])
            .unwrap();

        let (c1c, c1s) = result.recv().await.unwrap();
        let (c2c, c2s) = result.recv().await.unwrap();

        let c1t = c1c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        let c2t = c2c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c1t, vec![d(4)]);
        assert_eq!(c1s.previous_digest, None);
        assert_eq!(c1s.sequence_number, 0);

        assert_eq!(c2t, vec![d(3), d(2), d(1)]);
        assert_eq!(c2s.previous_digest, Some(c1s.digest()));
        assert_eq!(c2s.sequence_number, 1);
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

    impl CheckpointOutput for mpsc::Sender<(CheckpointContents, CheckpointSummary)> {
        fn checkpoint_created(
            &self,
            summary: &CheckpointSummary,
            contents: &CheckpointContents,
        ) -> SuiResult {
            self.try_send((contents.clone(), summary.clone())).unwrap();
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
    ) -> TransactionEffects {
        TransactionEffects {
            transaction_digest,
            dependencies,
            ..Default::default()
        }
    }
}
