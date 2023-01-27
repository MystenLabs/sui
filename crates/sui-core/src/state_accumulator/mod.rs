// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO(william) add metrics
// TODO(william) make configurable

use either::Either;
use futures::future::join_all;
use futures::stream::FuturesOrdered;

use sui_macros::nondeterministic;
use sui_types::base_types::{ObjectDigest, ObjectID};
use tracing::debug;
use typed_store::Map;

use std::path::Path;
use std::sync::Arc;

use fastcrypto::hash::MultisetHash;
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

type AccumulatorTaskBuffer = FuturesOrdered<JoinHandle<SuiResult<(Accumulator, State)>>>;
type EndOfEpochFlag = bool;

#[derive(Debug, Clone)]
pub struct State {
    pub effects: Vec<TransactionEffects>,
    pub checkpoint_seq_num: CheckpointSequenceNumber,
    pub epoch: EpochId,
    pub end_of_epoch_flag: EndOfEpochFlag,
}

#[derive(DBMapUtils)]
pub struct StateAccumulatorTables {
    // TODO: implement pruning policy as most of these tables can
    // be cleaned up at end of epoch

    // Maps checkpoint sequence number to an accumulator with accumulated state
    // only for the checkpoint that the key references. Append-only, i.e.,
    // the accumulator is complete wrt the checkpoint
    pub state_hash_by_checkpoint: DBMap<CheckpointSequenceNumber, Accumulator>,

    // A live, append-only table representing the running root state hash, computed at
    // arbitrary checkpoints. As such, should NEVER be relied upon for snapshot
    // verification or consensus.For key with Checkpoint C, the value is the accumulation
    // of all state from checkpoint 0..C
    pub(self) root_state_hash_by_checkpoint: DBMap<CheckpointSequenceNumber, Accumulator>,

    // Finalized root state accumulator for epoch, to be included in CheckpointSummary
    // of last checkpoint of epoch. These values should only ever be written once
    // and never changed
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
        // We need to register waiters _before_ reading from the database to avoid
        // race conditions
        let registrations = self
            .checkpoint_state_notify_read
            .register_all(checkpoints.clone());
        let accumulators = self
            .tables
            .state_hash_by_checkpoint
            .multi_get(checkpoints)?;

        // Zipping together registrations and accumulators ensures returned order is
        // the same as order of digests
        let results =
            accumulators
                .into_iter()
                .zip(registrations.into_iter())
                .map(|(a, r)| match a {
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
        let registration = self.root_state_notify_read.register_one(&epoch);
        let hash = self.tables.root_state_hash_by_epoch.get(&epoch)?;

        let result = match hash {
            // Note that Some() clause also drops registration that is already fulfilled
            Some(ready) => Either::Left(futures::future::ready(ready)),
            None => Either::Right(registration),
        }
        .await;

        Ok(result)
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
                        self.store.checkpoint_state_notify_read.notify(&state.checkpoint_seq_num, &acc);
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
                        self.accumulate(state)
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
    async fn accumulate(&self, last_state_of_epoch: State) -> Result<(), TypedStoreError> {
        let State {
            epoch,
            checkpoint_seq_num: last_checkpoint,
            ..
        } = last_state_of_epoch;

        if self
            .store
            .tables
            .root_state_hash_by_epoch
            .contains_key(&epoch)?
        {
            return Ok(());
        }

        let (next_to_accumulate, mut root_state_hash) = self
            .store
            .tables
            .root_state_hash_by_checkpoint
            .iter()
            .skip_to_last()
            .next()
            .map(|(highest, hash)| (highest.saturating_add(1), hash))
            .unwrap_or((0, Accumulator::default()));

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

        // We want to enforce that this table is append only, otherwise we
        // may end up re-accumulating the same state
        assert!(
            !self
                .store
                .tables
                .root_state_hash_by_checkpoint
                .contains_key(&last_checkpoint)?,
            "StateAccumulator: root state hash already exists for checkpoint {last_checkpoint:?}"
        );

        // Update root_state_hash tables atomically
        let batch = self.store.tables.root_state_hash_by_checkpoint.batch();

        let batch = batch.insert_batch(
            &self.store.tables.root_state_hash_by_checkpoint,
            std::iter::once((&last_checkpoint, root_state_hash.clone())),
        )?;
        let batch = batch.insert_batch(
            &self.store.tables.root_state_hash_by_epoch,
            std::iter::once((&epoch, root_state_hash.clone())),
        )?;

        batch.write()?;
        self.store
            .root_state_notify_read
            .notify(&epoch, &root_state_hash);

        Ok(())
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
