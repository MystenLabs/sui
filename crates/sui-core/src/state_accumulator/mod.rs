// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO(william) add metrics
// TODO(william) make configurable

use either::Either;
use futures::future::join_all;
use futures::stream::FuturesOrdered;

use sui_types::base_types::ObjectDigest;
use tracing::debug;
use typed_store::Map;

use std::path::Path;
use std::sync::Arc;

use fastcrypto::hash::{Digest, MultisetHash};
use mysten_metrics::spawn_monitored_task;
use sui_types::accumulator::Accumulator;
use sui_types::committee::EpochId;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::TransactionEffects;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use typed_store::rocks::TypedStoreError;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store_derive::DBMapUtils;

use crate::authority::authority_notify_read::NotifyRead;
use crate::checkpoints::CheckpointWatermark;

const ROOT_STATE_HASH_KEY: u64 = 0;

type AccumulatorTaskBuffer = FuturesOrdered<JoinHandle<SuiResult<(Accumulator, State)>>>;
type EndOfEpochFlag = bool;

#[derive(Debug, Clone)]
pub struct State {
    pub effects: Vec<TransactionEffects>,
    pub checkpoint_seq_num: CheckpointSequenceNumber,
    pub epoch: EpochId,
    pub end_of_epoch_flag: EndOfEpochFlag,
    pub(crate) checkpoint_sequence: u64,
}

#[derive(DBMapUtils)]
pub struct StateAccumulatorTables {
    // TODO: implement pruning policy as most of these tables can
    // be cleaned up at end of epoch
    watermarks: DBMap<CheckpointWatermark, CheckpointSequenceNumber>,

    pub state_hash_by_checkpoint: DBMap<CheckpointSequenceNumber, Accumulator>,

    // A live / hot object representing the running root state hash, computed from
    // the running accumulation of checkpoint state hashes. As such, should NEVER be
    // relied upon for snapshot verification or consensus.
    //
    // TODO: MUST BE THREAD SAFE
    pub(self) root_state_hash: DBMap<u64, Accumulator>,

    // Finalized root state digest for epoch, to be included in CheckpointSummary
    // of last checkpoint of epoch. These values should only ever be written once
    // and never changed
    // TODO(william) change value type to Accumulator. First may need to figure out
    // some serializability issues with Accumulator.
    pub root_state_hash_by_epoch: DBMap<EpochId, Accumulator>,
}

impl StateAccumulatorTables {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::default(),
            None,
            None,
        ))
    }

    pub fn get_state_hash_for_checkpoint(
        &self,
        checkpoint: &CheckpointSequenceNumber,
    ) -> Result<Option<Accumulator>, TypedStoreError> {
        self.state_hash_by_checkpoint.get(checkpoint)
    }

    pub fn get_most_recent_root_state_hash(&self) -> Result<Option<Accumulator>, TypedStoreError> {
        self.root_state_hash.get(&ROOT_STATE_HASH_KEY)
    }

    /// Idempotent because checkpoints are final. Moreover, we will
    /// often have the same checkpoint forwarded from different
    /// components (CheckpointOutput/CheckpointExecutor) and do not
    /// want to double count against the root state hash.
    pub fn insert_state_hash_for_checkpoint(
        &self,
        state_hash: Accumulator,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<(), TypedStoreError> {
        if self.state_hash_by_checkpoint.contains_key(&checkpoint)? {
            Ok(())
        } else {
            self.state_hash_by_checkpoint
                .insert(&checkpoint, &state_hash)
        }
    }

    pub fn get_highest_accumulated_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        if let Some(highest_accumulated) = self
            .watermarks
            .get(&CheckpointWatermark::HighestAccumulated)?
        {
            Ok(Some(highest_accumulated))
        } else {
            Ok(None)
        }
    }

    pub fn update_highest_accumulated_checkpoint_seq_number(
        &self,
        checkpoint_seq_num: CheckpointSequenceNumber,
    ) -> Result<(), TypedStoreError> {
        match self.get_highest_accumulated_checkpoint_seq_number()? {
            Some(old_seq_number) if old_seq_number > checkpoint_seq_num => Ok(()),
            _ => self.watermarks.insert(
                &CheckpointWatermark::HighestAccumulated,
                &checkpoint_seq_num,
            ),
        }
    }
}

pub struct StateAccumulatorStore {
    pub tables: Arc<StateAccumulatorTables>,
    // Implementation details to support notify_read
    pub(crate) checkpoint_state_notify_read: NotifyRead<CheckpointSequenceNumber, Accumulator>,

    pub(crate) root_state_notify_read: NotifyRead<EpochId, Accumulator>,
}

impl StateAccumulatorStore {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(StateAccumulatorStore {
            tables: StateAccumulatorTables::new(path),
            checkpoint_state_notify_read: NotifyRead::new(),
            root_state_notify_read: NotifyRead::new(),
        })
    }

    /// Returns future containing the state digest for the given epoch
    /// once available
    pub async fn notify_read_checkpoint_state_digests(
        &self,
        checkpoints: Vec<CheckpointSequenceNumber>,
    ) -> SuiResult<Vec<Accumulator>> {
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registrations = self
            .checkpoint_state_notify_read
            .register_all(checkpoints.clone());
        let accumulators = self
            .tables
            .state_hash_by_checkpoint
            .multi_get(checkpoints)?;
        // Zipping together registrations and effects ensures returned order is the same as order of digests
        let results =
            accumulators
                .into_iter()
                .zip(registrations.into_iter())
                .map(|(e, r)| match e {
                    // Note that Some() clause also drops registration that is already fulfilled
                    Some(ready) => Either::Left(futures::future::ready(ready)),
                    None => Either::Right(r),
                });

        Ok(join_all(results).await)
    }

    /// Returns future containing the state hash for the given epoch
    /// once available
    pub async fn notify_read_root_state_hash(&self, epoch: EpochId) -> SuiResult<Accumulator> {
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registrations = self
            .root_state_notify_read
            .register_all(vec![epoch.clone()]);
        let hashes = self
            .tables
            .root_state_hash_by_epoch
            .multi_get(vec![epoch.clone()])?;
        // Zipping together registrations and effects ensures returned order is the same as order of digests
        let results = hashes
            .into_iter()
            .zip(registrations.into_iter())
            .map(|(e, r)| match e {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => Either::Right(r),
            });
        let mut results = join_all(results).await;
        assert_eq!(
            results.len(),
            1,
            "Expected exactly one root state digest from notify_read"
        );

        Ok(results.pop().unwrap())
    }
}

/// Thread safe clone-able handle for interacting with running StateAccumulator
#[derive(Clone)]
pub struct StateAccumulatorService {
    sender: mpsc::Sender<State>,
    pub store: Arc<StateAccumulatorStore>,
}

impl StateAccumulatorService {
    /// Enqueue checkpoint state for accumulation. This operation
    /// must be idempotent, as the same checkpoint may be enqueued
    /// by different components at arbitrary times
    pub async fn enqueue(&self, state: State) -> SuiResult {
        self.sender.send(state).await.map_err(|err| {
            SuiError::from(format!("Blocking enqueue on StateAccumulator failed: {err:?}").as_str())
        })?;

        Ok(())
    }
}

pub struct StateAccumulator {
    store: Arc<StateAccumulatorStore>,
    queue: mpsc::Receiver<State>,
    sender: mpsc::Sender<State>,
}

impl StateAccumulator {
    pub fn new(queue_size: u64, store_path: &Path) -> Self {
        let store = StateAccumulatorStore::new(store_path);

        // in practice we want the queue_size to be at least 1 greater than
        // the max concurrency for CheckpointExecutor. This should ensure that
        // we don't block on enqueueing
        let (sender, receiver) = mpsc::channel(queue_size as usize);

        Self {
            store,
            queue: receiver,
            sender,
        }
    }

    pub fn new_for_tests(queue_size: u64) -> Self {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
        std::fs::create_dir(&path).unwrap();
        Self::new(queue_size, &path.join("store"))
    }

    pub async fn start(mut self) -> (StateAccumulatorService, JoinHandle<()>) {
        let sender = self.sender.clone();
        let store = self.store.clone();
        let handle = spawn_monitored_task!(self.run());

        (StateAccumulatorService { sender, store }, handle)
    }

    /// Reads checkpoints from the queue and manages parallel tasks (one
    /// per checkpoint) to gen and save the checkpoint state hash. Once
    /// end of epoch checkpoint is processed, creates a root state digest
    async fn run(&mut self) {
        let mut pending: AccumulatorTaskBuffer = FuturesOrdered::new();

        loop {
            tokio::select! {
                // process completed tasks
                Some(Ok(Ok((acc, state)))) = pending.next() => {
                    // Save checkpoint hash if not already exists
                    if !self.store.tables.state_hash_by_checkpoint.contains_key(&state.checkpoint_seq_num).unwrap() {
                        self.store.tables.state_hash_by_checkpoint
                            .insert(&state.checkpoint_seq_num, &acc)
                            .expect("StateAccumulator: failed to insert state hash");
                    }

                    if state.end_of_epoch_flag {
                        // TODOS:
                        //
                        // 1. Here we call this once at the end of the epoch. Depending
                        // on the performance here, we may instead move to running this as
                        // a separate background task that accumulates on an ongoing basis.
                        //
                        // 2. Here was assume for simplicity that the end of epoch checkpoint will always
                        // be scheduled last. This is true for now, but we should not assume this.
                        // Can be fixed to spawn a separate process that does a best effort root digest
                        // accumulation, with retries in case there are missing checkpoint accumulators.
                        self.accumulate(state.checkpoint_seq_num, state.epoch)
                            .await
                            .expect("Accumulation failed");
                    }
                }
                // schedule new tasks
                state = self.queue.recv() => match state {
                    Some(state) => {
                        debug!(
                            ?state,
                            "StateAccumulator: received enqueued state",
                        );

                        let store = self.store.clone();
                        let state_clone = state.clone();

                        pending.push_back(spawn_monitored_task!(async move {
                            let accumulator = hash_checkpoint_effects(state_clone.clone(), store)?;
                            Ok((accumulator, state_clone))
                        }));
                    }
                    None => panic!("StateAccumulator: all senders have been dropped"),
                }
            }
        }
    }

    /// Unions all checkpoint accumulators at the end of the epoch to generate the
    /// root state hash and saves it. This function is guaranteed to be idempotent (despite the
    /// underlying data structure not being) as long as it is not called in a multi-threaded
    /// context.
    async fn accumulate(
        &self,
        epoch: EpochId,
        last_checkpoint: CheckpointSequenceNumber,
    ) -> Result<(), TypedStoreError> {
        if self
            .store
            .tables
            .root_state_hash_by_epoch
            .contains_key(&epoch)?
        {
            return Ok(());
        }

        let next_to_accumulate = self
            .store
            .tables
            .get_highest_accumulated_checkpoint_seq_number()?
            .map(|num| num.saturating_add(1))
            .unwrap_or(0);

        let mut root_state_hash = self
            .store
            .tables
            .root_state_hash
            .get(&ROOT_STATE_HASH_KEY)?
            .unwrap_or_default();

        for i in next_to_accumulate..=last_checkpoint {
            let acc = self
                .store
                .tables
                .state_hash_by_checkpoint
                .get(&i)?
                .unwrap_or_else(|| {
                    panic!("Accumulator for checkpoint sequence number {i:?} not present in store")
                });
            root_state_hash.union(&acc);
        }

        // Update watermark, root_state_hash, and digest atomically
        let batch = self.store.tables.root_state_hash.batch();

        let batch = batch.insert_batch(
            &self.store.tables.root_state_hash,
            std::iter::once((&ROOT_STATE_HASH_KEY, root_state_hash.clone())),
        )?;
        let batch = batch.insert_batch(
            &self.store.tables.root_state_hash_by_epoch,
            std::iter::once((&epoch, root_state_hash)),
        )?;

        let batch = match self
            .store
            .tables
            .get_highest_accumulated_checkpoint_seq_number()?
        {
            Some(old_seq_number) if old_seq_number > last_checkpoint => batch,
            _ => batch.insert_batch(
                &self.store.tables.watermarks,
                std::iter::once((&CheckpointWatermark::HighestAccumulated, last_checkpoint)),
            )?,
        };

        batch.write()
    }
}

fn hash_checkpoint_effects(
    state: State,
    store: Arc<StateAccumulatorStore>,
) -> SuiResult<Accumulator> {
    let seq_num = state.checkpoint_seq_num;

    if store
        .tables
        .state_hash_by_checkpoint
        .contains_key(&seq_num)
        .unwrap()
    {
        return Err(SuiError::from(
            format!("StateAccumulator: checkpoint already hashed: {seq_num:?}").as_str(),
        ));
    }

    let mut acc = Accumulator::default();

    acc.insert_all(
        state
            .effects
            .iter()
            .flat_map(|fx| {
                fx.created
                    .clone()
                    .into_iter()
                    .map(|(obj_ref, _)| obj_ref.2)
                    .collect::<Vec<ObjectDigest>>()
            })
            .collect::<Vec<ObjectDigest>>(),
    );
    acc.remove_all(
        state
            .effects
            .iter()
            .flat_map(|fx| {
                fx.deleted
                    .clone()
                    .into_iter()
                    .map(|obj_ref| obj_ref.2)
                    .collect::<Vec<ObjectDigest>>()
            })
            .collect::<Vec<ObjectDigest>>(),
    );
    // TODO almost certainly not currectly handling "mutated" effects. Help?
    acc.insert_all(
        state
            .effects
            .iter()
            .flat_map(|fx| {
                fx.mutated
                    .clone()
                    .into_iter()
                    .map(|(obj_ref, _)| obj_ref.2)
                    .collect::<Vec<ObjectDigest>>()
            })
            .collect::<Vec<ObjectDigest>>(),
    );
    Ok(acc)
}
