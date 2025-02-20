// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::{stream::FuturesUnordered, StreamExt as _};
use itertools::Itertools as _;
use mysten_metrics::{
    monitored_future,
    monitored_mpsc::{channel, Receiver, Sender},
    monitored_scope,
};
use parking_lot::{Mutex, RwLock};
use rand::{prelude::SliceRandom as _, rngs::ThreadRng};
use sui_macros::fail_point_async;
use tap::TapFallible;
use tokio::{
    runtime::Handle,
    sync::{mpsc::error::TrySendError, oneshot},
    task::{JoinError, JoinSet},
    time::{sleep, sleep_until, timeout, Instant},
};
use tracing::{debug, error, info, trace, warn};

use crate::{authority_service::COMMIT_LAG_MULTIPLIER, core_thread::CoreThreadDispatcher};
use crate::{
    block::{BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    commit_vote_monitor::CommitVoteMonitor,
    context::Context,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::NetworkClient,
    BlockAPI, CommitIndex, Round,
};

/// The number of concurrent fetch blocks requests per authority
const FETCH_BLOCKS_CONCURRENCY: usize = 5;

/// Timeouts when fetching blocks.
const FETCH_REQUEST_TIMEOUT: Duration = Duration::from_millis(2_000);
const FETCH_FROM_PEERS_TIMEOUT: Duration = Duration::from_millis(4_000);

/// Max number of blocks to fetch per request.
/// This value should be chosen so even with blocks at max size, the requests
/// can finish on hosts with good network using the timeouts above.
const MAX_BLOCKS_PER_FETCH: usize = 32;

const MAX_AUTHORITIES_TO_FETCH_PER_BLOCK: usize = 2;

/// The number of rounds above the highest accepted round that still willing to fetch missing blocks via the periodic
/// synchronizer. Any missing blocks of higher rounds are considered too far in the future to fetch. This property is taken into
/// account only when it's detected that the node has fallen behind on its commit compared to the rest of the network, otherwise
/// scheduler will attempt to fetch any missing block.
const SYNC_MISSING_BLOCK_ROUND_THRESHOLD: u32 = 50;

struct BlocksGuard {
    map: Arc<InflightBlocksMap>,
    block_refs: BTreeSet<BlockRef>,
    peer: AuthorityIndex,
}

impl Drop for BlocksGuard {
    fn drop(&mut self) {
        self.map.unlock_blocks(&self.block_refs, self.peer);
    }
}

// Keeps a mapping between the missing blocks that have been instructed to be fetched and the authorities
// that are currently fetching them. For a block ref there is a maximum number of authorities that can
// concurrently fetch it. The authority ids that are currently fetching a block are set on the corresponding
// `BTreeSet` and basically they act as "locks".
struct InflightBlocksMap {
    inner: Mutex<HashMap<BlockRef, BTreeSet<AuthorityIndex>>>,
}

impl InflightBlocksMap {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// Locks the blocks to be fetched for the assigned `peer_index`. We want to avoid re-fetching the
    /// missing blocks from too many authorities at the same time, thus we limit the concurrency
    /// per block by attempting to lock per block. If a block is already fetched by the maximum allowed
    /// number of authorities, then the block ref will not be included in the returned set. The method
    /// returns all the block refs that have been successfully locked and allowed to be fetched.
    fn lock_blocks(
        self: &Arc<Self>,
        missing_block_refs: BTreeSet<BlockRef>,
        peer: AuthorityIndex,
    ) -> Option<BlocksGuard> {
        let mut blocks = BTreeSet::new();
        let mut inner = self.inner.lock();

        for block_ref in missing_block_refs {
            // check that the number of authorities that are already instructed to fetch the block is not
            // higher than the allowed and the `peer_index` has not already been instructed to do that.
            let authorities = inner.entry(block_ref).or_default();
            if authorities.len() < MAX_AUTHORITIES_TO_FETCH_PER_BLOCK
                && authorities.get(&peer).is_none()
            {
                assert!(authorities.insert(peer));
                blocks.insert(block_ref);
            }
        }

        if blocks.is_empty() {
            None
        } else {
            Some(BlocksGuard {
                map: self.clone(),
                block_refs: blocks,
                peer,
            })
        }
    }

    /// Unlocks the provided block references for the given `peer`. The unlocking is strict, meaning that
    /// if this method is called for a specific block ref and peer more times than the corresponding lock
    /// has been called, it will panic.
    fn unlock_blocks(self: &Arc<Self>, block_refs: &BTreeSet<BlockRef>, peer: AuthorityIndex) {
        // Now mark all the blocks as fetched from the map
        let mut blocks_to_fetch = self.inner.lock();
        for block_ref in block_refs {
            let authorities = blocks_to_fetch
                .get_mut(block_ref)
                .expect("Should have found a non empty map");

            assert!(authorities.remove(&peer), "Peer index should be present!");

            // if the last one then just clean up
            if authorities.is_empty() {
                blocks_to_fetch.remove(block_ref);
            }
        }
    }

    /// Drops the provided `blocks_guard` which will force to unlock the blocks, and lock now again the
    /// referenced block refs. The swap is best effort and there is no guarantee that the `peer` will
    /// be able to acquire the new locks.
    fn swap_locks(
        self: &Arc<Self>,
        blocks_guard: BlocksGuard,
        peer: AuthorityIndex,
    ) -> Option<BlocksGuard> {
        let block_refs = blocks_guard.block_refs.clone();

        // Explicitly drop the guard
        drop(blocks_guard);

        // Now create new guard
        self.lock_blocks(block_refs, peer)
    }

    #[cfg(test)]
    fn num_of_locked_blocks(self: &Arc<Self>) -> usize {
        let inner = self.inner.lock();
        inner.len()
    }
}

enum Command {
    FetchBlocks {
        missing_block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
        result: oneshot::Sender<Result<(), ConsensusError>>,
    },
    FetchOwnLastBlock,
    KickOffScheduler,
}

pub(crate) struct SynchronizerHandle {
    commands_sender: Sender<Command>,
    tasks: tokio::sync::Mutex<JoinSet<()>>,
}

impl SynchronizerHandle {
    /// Explicitly asks from the synchronizer to fetch the blocks - provided the block_refs set - from
    /// the peer authority.
    pub(crate) async fn fetch_blocks(
        &self,
        missing_block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
    ) -> ConsensusResult<()> {
        let (sender, receiver) = oneshot::channel();
        self.commands_sender
            .send(Command::FetchBlocks {
                missing_block_refs,
                peer_index,
                result: sender,
            })
            .await
            .map_err(|_err| ConsensusError::Shutdown)?;
        receiver.await.map_err(|_err| ConsensusError::Shutdown)?
    }

    pub(crate) async fn stop(&self) -> Result<(), JoinError> {
        let mut tasks = self.tasks.lock().await;
        tasks.abort_all();
        while let Some(result) = tasks.join_next().await {
            result?
        }
        Ok(())
    }
}

/// `Synchronizer` oversees live block synchronization, crucial for node progress. Live synchronization
/// refers to the process of retrieving missing blocks, particularly those essential for advancing a node
/// when data from only a few rounds is absent. If a node significantly lags behind the network,
/// `commit_syncer` handles fetching missing blocks via a more efficient approach. `Synchronizer`
/// aims for swift catch-up employing two mechanisms:
///
/// 1. Explicitly requesting missing blocks from designated authorities via the "block send" path.
///    This includes attempting to fetch any missing ancestors necessary for processing a received block.
///    Such requests prioritize the block author, maximizing the chance of prompt retrieval.
///    A locking mechanism allows concurrent requests for missing blocks from up to two authorities
///    simultaneously, enhancing the chances of timely retrieval. Notably, if additional missing blocks
///    arise during block processing, requests to the same authority are deferred to the scheduler.
///
/// 2. Periodically requesting missing blocks via a scheduler. This primarily serves to retrieve
///    missing blocks that were not ancestors of a received block via the "block send" path.
///    The scheduler operates on either a fixed periodic basis or is triggered immediately
///    after explicit fetches described in (1), ensuring continued block retrieval if gaps persist.
///
/// Additionally to the above, the synchronizer can synchronize and fetch the last own proposed block
/// from the network peers as best effort approach to recover node from amnesia and avoid making the
/// node equivocate.
pub(crate) struct Synchronizer<C: NetworkClient, V: BlockVerifier, D: CoreThreadDispatcher> {
    context: Arc<Context>,
    commands_receiver: Receiver<Command>,
    fetch_block_senders: BTreeMap<AuthorityIndex, Sender<BlocksGuard>>,
    core_dispatcher: Arc<D>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    dag_state: Arc<RwLock<DagState>>,
    fetch_blocks_scheduler_task: JoinSet<()>,
    fetch_own_last_block_task: JoinSet<()>,
    network_client: Arc<C>,
    block_verifier: Arc<V>,
    inflight_blocks_map: Arc<InflightBlocksMap>,
    commands_sender: Sender<Command>,
}

impl<C: NetworkClient, V: BlockVerifier, D: CoreThreadDispatcher> Synchronizer<C, V, D> {
    pub fn start(
        network_client: Arc<C>,
        context: Arc<Context>,
        core_dispatcher: Arc<D>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        block_verifier: Arc<V>,
        dag_state: Arc<RwLock<DagState>>,
        sync_last_known_own_block: bool,
    ) -> Arc<SynchronizerHandle> {
        let (commands_sender, commands_receiver) =
            channel("consensus_synchronizer_commands", 1_000);
        let inflight_blocks_map = InflightBlocksMap::new();

        // Spawn the tasks to fetch the blocks from the others
        let mut fetch_block_senders = BTreeMap::new();
        let mut tasks = JoinSet::new();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            let (sender, receiver) =
                channel("consensus_synchronizer_fetches", FETCH_BLOCKS_CONCURRENCY);
            let fetch_blocks_from_authority_async = Self::fetch_blocks_from_authority(
                index,
                network_client.clone(),
                block_verifier.clone(),
                commit_vote_monitor.clone(),
                context.clone(),
                core_dispatcher.clone(),
                dag_state.clone(),
                receiver,
                commands_sender.clone(),
            );
            tasks.spawn(monitored_future!(fetch_blocks_from_authority_async));
            fetch_block_senders.insert(index, sender);
        }

        let commands_sender_clone = commands_sender.clone();

        if sync_last_known_own_block {
            commands_sender
                .try_send(Command::FetchOwnLastBlock)
                .expect("Failed to sync our last block");
        }

        // Spawn the task to listen to the requests & periodic runs
        tasks.spawn(monitored_future!(async move {
            let mut s = Self {
                context,
                commands_receiver,
                fetch_block_senders,
                core_dispatcher,
                commit_vote_monitor,
                fetch_blocks_scheduler_task: JoinSet::new(),
                fetch_own_last_block_task: JoinSet::new(),
                network_client,
                block_verifier,
                inflight_blocks_map,
                commands_sender: commands_sender_clone,
                dag_state,
            };
            s.run().await;
        }));

        Arc::new(SynchronizerHandle {
            commands_sender,
            tasks: tokio::sync::Mutex::new(tasks),
        })
    }

    // The main loop to listen for the submitted commands.
    async fn run(&mut self) {
        // We want the synchronizer to run periodically every 500ms to fetch any missing blocks.
        const SYNCHRONIZER_TIMEOUT: Duration = Duration::from_millis(500);
        let scheduler_timeout = sleep_until(Instant::now() + SYNCHRONIZER_TIMEOUT);

        tokio::pin!(scheduler_timeout);

        loop {
            tokio::select! {
                Some(command) = self.commands_receiver.recv() => {
                    match command {
                        Command::FetchBlocks{ missing_block_refs, peer_index, result } => {
                            if peer_index == self.context.own_index {
                                error!("We should never attempt to fetch blocks from our own node");
                                continue;
                            }

                            // Keep only the max allowed blocks to request. It is ok to reduce here as the scheduler
                            // task will take care syncing whatever is leftover.
                            let missing_block_refs = missing_block_refs
                                .into_iter()
                                .take(MAX_BLOCKS_PER_FETCH)
                                .collect();

                            let blocks_guard = self.inflight_blocks_map.lock_blocks(missing_block_refs, peer_index);
                            let Some(blocks_guard) = blocks_guard else {
                                result.send(Ok(())).ok();
                                continue;
                            };

                            // We don't block if the corresponding peer task is saturated - but we rather drop the request. That's ok as the periodic
                            // synchronization task will handle any still missing blocks in next run.
                            let r = self
                                .fetch_block_senders
                                .get(&peer_index)
                                .expect("Fatal error, sender should be present")
                                .try_send(blocks_guard)
                                .map_err(|err| {
                                    match err {
                                        TrySendError::Full(_) => ConsensusError::SynchronizerSaturated(peer_index),
                                        TrySendError::Closed(_) => ConsensusError::Shutdown
                                    }
                                });

                            result.send(r).ok();
                        }
                        Command::FetchOwnLastBlock => {
                            if self.fetch_own_last_block_task.is_empty() {
                                self.start_fetch_own_last_block_task();
                            }
                        }
                        Command::KickOffScheduler => {
                            // just reset the scheduler timeout timer to run immediately if not already running.
                            // If the scheduler is already running then just reduce the remaining time to run.
                            let timeout = if self.fetch_blocks_scheduler_task.is_empty() {
                                Instant::now()
                            } else {
                                Instant::now() + SYNCHRONIZER_TIMEOUT.checked_div(2).unwrap()
                            };

                            // only reset if it is earlier than the next deadline
                            if timeout < scheduler_timeout.deadline() {
                                scheduler_timeout.as_mut().reset(timeout);
                            }
                        }
                    }
                },
                Some(result) = self.fetch_own_last_block_task.join_next(), if !self.fetch_own_last_block_task.is_empty() => {
                    match result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                            } else if e.is_panic() {
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("fetch our last block task failed: {e}");
                            }
                        },
                    };
                },
                Some(result) = self.fetch_blocks_scheduler_task.join_next(), if !self.fetch_blocks_scheduler_task.is_empty() => {
                    match result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                            } else if e.is_panic() {
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("fetch blocks scheduler task failed: {e}");
                            }
                        },
                    };
                },
                () = &mut scheduler_timeout => {
                    // we want to start a new task only if the previous one has already finished.
                    if self.fetch_blocks_scheduler_task.is_empty() {
                        if let Err(err) = self.start_fetch_missing_blocks_task().await {
                            debug!("Core is shutting down, synchronizer is shutting down: {err:?}");
                            return;
                        };
                    }

                    scheduler_timeout
                        .as_mut()
                        .reset(Instant::now() + SYNCHRONIZER_TIMEOUT);
                }
            }
        }
    }

    async fn fetch_blocks_from_authority(
        peer_index: AuthorityIndex,
        network_client: Arc<C>,
        block_verifier: Arc<V>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        context: Arc<Context>,
        core_dispatcher: Arc<D>,
        dag_state: Arc<RwLock<DagState>>,
        mut receiver: Receiver<BlocksGuard>,
        commands_sender: Sender<Command>,
    ) {
        const MAX_RETRIES: u32 = 5;
        let peer_hostname = &context.committee.authority(peer_index).hostname;
        let mut requests = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(blocks_guard) = receiver.recv(), if requests.len() < FETCH_BLOCKS_CONCURRENCY => {
                    // get the highest accepted rounds
                    let highest_rounds = Self::get_highest_accepted_rounds(dag_state.clone(), &context);

                    requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, blocks_guard, highest_rounds, FETCH_REQUEST_TIMEOUT, 1))
                },
                Some((response, blocks_guard, retries, _peer, highest_rounds)) = requests.next() => {
                    match response {
                        Ok(blocks) => {
                            if let Err(err) = Self::process_fetched_blocks(blocks,
                                peer_index,
                                blocks_guard,
                                core_dispatcher.clone(),
                                block_verifier.clone(),
                                commit_vote_monitor.clone(),
                                context.clone(),
                                commands_sender.clone(),
                                "live"
                            ).await {
                                warn!("Error while processing fetched blocks from peer {peer_index} {peer_hostname}: {err}");
                            }
                        },
                        Err(_) => {
                            if retries <= MAX_RETRIES {
                                requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, blocks_guard, highest_rounds, FETCH_REQUEST_TIMEOUT, retries))
                            } else {
                                warn!("Max retries {retries} reached while trying to fetch blocks from peer {peer_index} {peer_hostname}.");
                                // we don't necessarily need to do, but dropping the guard here to unlock the blocks
                                drop(blocks_guard);
                            }
                        }
                    }
                },
                else => {
                    info!("Fetching blocks from authority {peer_index} task will now abort.");
                    break;
                }
            }
        }
    }

    /// Processes the requested raw fetched blocks from peer `peer_index`. If no error is returned then
    /// the verified blocks are immediately sent to Core for processing.
    async fn process_fetched_blocks(
        serialized_blocks: Vec<Bytes>,
        peer_index: AuthorityIndex,
        requested_blocks_guard: BlocksGuard,
        core_dispatcher: Arc<D>,
        block_verifier: Arc<V>,
        commit_vote_monitor: Arc<CommitVoteMonitor>,
        context: Arc<Context>,
        commands_sender: Sender<Command>,
        sync_method: &str,
    ) -> ConsensusResult<()> {
        // The maximum number of blocks that can be additionally fetched from the one requested - those
        // are potentially missing ancestors.
        const MAX_ADDITIONAL_BLOCKS: usize = 10;

        // Ensure that all the returned blocks do not go over the total max allowed returned blocks
        if serialized_blocks.len() > requested_blocks_guard.block_refs.len() + MAX_ADDITIONAL_BLOCKS
        {
            return Err(ConsensusError::TooManyFetchedBlocksReturned(peer_index));
        }

        // Verify all the fetched blocks
        let blocks = Handle::current()
            .spawn_blocking({
                let block_verifier = block_verifier.clone();
                let context = context.clone();
                move || Self::verify_blocks(serialized_blocks, block_verifier, &context, peer_index)
            })
            .await
            .expect("Spawn blocking should not fail")?;

        // Get all the ancestors of the requested blocks only
        let ancestors = blocks
            .iter()
            .filter(|b| requested_blocks_guard.block_refs.contains(&b.reference()))
            .flat_map(|b| b.ancestors().to_vec())
            .collect::<BTreeSet<BlockRef>>();

        // Now confirm that the blocks are either between the ones requested, or they are parents of the requested blocks
        for block in &blocks {
            if !requested_blocks_guard
                .block_refs
                .contains(&block.reference())
                && !ancestors.contains(&block.reference())
            {
                return Err(ConsensusError::UnexpectedFetchedBlock {
                    index: peer_index,
                    block_ref: block.reference(),
                });
            }
        }

        // Record commit votes from the verified blocks.
        for block in &blocks {
            commit_vote_monitor.observe_block(block);
        }

        let metrics = &context.metrics.node_metrics;
        let peer_hostname = &context.committee.authority(peer_index).hostname;
        metrics
            .synchronizer_fetched_blocks_by_peer
            .with_label_values(&[peer_hostname, sync_method])
            .inc_by(blocks.len() as u64);
        for block in &blocks {
            let block_hostname = &context.committee.authority(block.author()).hostname;
            metrics
                .synchronizer_fetched_blocks_by_authority
                .with_label_values(&[block_hostname, sync_method])
                .inc();
        }

        debug!(
            "Synced {} missing blocks from peer {peer_index} {peer_hostname}: {}",
            blocks.len(),
            blocks.iter().map(|b| b.reference().to_string()).join(", "),
        );

        // Now send them to core for processing. Ignore the returned missing blocks as we don't want
        // this mechanism to keep feedback looping on fetching more blocks. The periodic synchronization
        // will take care of that.
        let missing_blocks = core_dispatcher
            .add_blocks(blocks)
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        // now release all the locked blocks as they have been fetched, verified & processed
        drop(requested_blocks_guard);

        // kick off immediately the scheduled synchronizer
        if !missing_blocks.is_empty() {
            // do not block here, so we avoid any possible cycles.
            if let Err(TrySendError::Full(_)) = commands_sender.try_send(Command::KickOffScheduler)
            {
                warn!("Commands channel is full")
            }
        }

        context
            .metrics
            .node_metrics
            .missing_blocks_after_fetch_total
            .inc_by(missing_blocks.len() as u64);

        Ok(())
    }

    fn get_highest_accepted_rounds(
        dag_state: Arc<RwLock<DagState>>,
        context: &Arc<Context>,
    ) -> Vec<Round> {
        let blocks = dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX);
        assert_eq!(blocks.len(), context.committee.size());

        blocks
            .into_iter()
            .map(|(block, _)| block.round())
            .collect::<Vec<_>>()
    }

    fn verify_blocks(
        serialized_blocks: Vec<Bytes>,
        block_verifier: Arc<V>,
        context: &Context,
        peer_index: AuthorityIndex,
    ) -> ConsensusResult<Vec<VerifiedBlock>> {
        let mut verified_blocks = Vec::new();

        for serialized_block in serialized_blocks {
            let signed_block: SignedBlock =
                bcs::from_bytes(&serialized_block).map_err(ConsensusError::MalformedBlock)?;

            // TODO: cache received and verified block refs to avoid duplicated work.
            let verification_result = block_verifier.verify_and_vote(&signed_block);
            if let Err(e) = verification_result {
                // TODO: we might want to use a different metric to track the invalid "served" blocks
                // from the invalid "proposed" ones.
                let hostname = context.committee.authority(peer_index).hostname.clone();
                context
                    .metrics
                    .node_metrics
                    .invalid_blocks
                    .with_label_values(&[&hostname, "synchronizer", e.clone().name()])
                    .inc();
                warn!("Invalid block received from {}: {}", peer_index, e);
                return Err(e);
            }
            let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);

            // Dropping is ok because the block will be refetched.
            // TODO: improve efficiency, maybe suspend and continue processing the block asynchronously.
            let now = context.clock.timestamp_utc_ms();
            if now < verified_block.timestamp_ms() {
                warn!(
                    "Synced block {} timestamp {} is in the future (now={}). Ignoring.",
                    verified_block.reference(),
                    verified_block.timestamp_ms(),
                    now
                );
                continue;
            }

            verified_blocks.push(verified_block);
        }

        Ok(verified_blocks)
    }

    async fn fetch_blocks_request(
        network_client: Arc<C>,
        peer: AuthorityIndex,
        blocks_guard: BlocksGuard,
        highest_rounds: Vec<Round>,
        request_timeout: Duration,
        mut retries: u32,
    ) -> (
        ConsensusResult<Vec<Bytes>>,
        BlocksGuard,
        u32,
        AuthorityIndex,
        Vec<Round>,
    ) {
        let start = Instant::now();
        let resp = timeout(
            request_timeout,
            network_client.fetch_blocks(
                peer,
                blocks_guard
                    .block_refs
                    .clone()
                    .into_iter()
                    .collect::<Vec<_>>(),
                highest_rounds.clone().into_iter().collect::<Vec<_>>(),
                request_timeout,
            ),
        )
        .await;

        fail_point_async!("consensus-delay");

        let resp = match resp {
            Ok(Err(err)) => {
                // Add a delay before retrying - if that is needed. If request has timed out then eventually
                // this will be a no-op.
                sleep_until(start + request_timeout).await;
                retries += 1;
                Err(err)
            } // network error
            Err(err) => {
                // timeout
                sleep_until(start + request_timeout).await;
                retries += 1;
                Err(ConsensusError::NetworkRequestTimeout(err.to_string()))
            }
            Ok(result) => result,
        };
        (resp, blocks_guard, retries, peer, highest_rounds)
    }

    fn start_fetch_own_last_block_task(&mut self) {
        const FETCH_OWN_BLOCK_RETRY_DELAY: Duration = Duration::from_millis(1_000);
        const MAX_RETRY_DELAY_STEP: Duration = Duration::from_millis(4_000);

        let context = self.context.clone();
        let dag_state = self.dag_state.clone();
        let network_client = self.network_client.clone();
        let block_verifier = self.block_verifier.clone();
        let core_dispatcher = self.core_dispatcher.clone();

        self.fetch_own_last_block_task
            .spawn(monitored_future!(async move {
                let _scope = monitored_scope("FetchOwnLastBlockTask");

                let fetch_own_block = |authority_index: AuthorityIndex, fetch_own_block_delay: Duration| {
                    let network_client_cloned = network_client.clone();
                    let own_index = context.own_index;
                    async move {
                        sleep(fetch_own_block_delay).await;
                        let r = network_client_cloned.fetch_latest_blocks(authority_index, vec![own_index], FETCH_REQUEST_TIMEOUT).await;
                        (r, authority_index)
                    }
                };

                let process_blocks = |blocks: Vec<Bytes>, authority_index: AuthorityIndex| -> ConsensusResult<Vec<VerifiedBlock>> {
                    let mut result = Vec::new();
                    for serialized_block in blocks {
                        let signed_block = bcs::from_bytes(&serialized_block).map_err(ConsensusError::MalformedBlock)?;
                        block_verifier.verify_and_vote(&signed_block).tap_err(|err|{
                            let hostname = context.committee.authority(authority_index).hostname.clone();
                            context
                                .metrics
                                .node_metrics
                                .invalid_blocks
                                .with_label_values(&[&hostname, "synchronizer_own_block", err.clone().name()])
                                .inc();
                            warn!("Invalid block received from {}: {}", authority_index, err);
                        })?;

                        let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);
                        if verified_block.author() != context.own_index {
                            return Err(ConsensusError::UnexpectedLastOwnBlock { index: authority_index, block_ref: verified_block.reference()});
                        }
                        result.push(verified_block);
                    }
                    Ok(result)
                };

                // Get the highest of all the results. Retry until at least `f+1` results have been gathered.
                let mut highest_round;
                let mut retries = 0;
                let mut retry_delay_step = Duration::from_millis(500);
                'main:loop {
                    if context.committee.size() == 1 {
                        highest_round = dag_state.read().get_last_proposed_block().round();
                        info!("Only one node in the network, will not try fetching own last block from peers.");
                        break 'main;
                    }

                    let mut total_stake = 0;
                    highest_round = 0;

                    // Ask all the other peers about our last block
                    let mut results = FuturesUnordered::new();

                    for (authority_index, _authority) in context.committee.authorities() {
                        if authority_index != context.own_index {
                            results.push(fetch_own_block(authority_index, Duration::from_millis(0)));
                        }
                    }

                    // Gather the results but wait to timeout as well
                    let timer = sleep_until(Instant::now() + context.parameters.sync_last_known_own_block_timeout);
                    tokio::pin!(timer);

                    'inner: loop {
                        tokio::select! {
                            result = results.next() => {
                                let Some((result, authority_index)) = result else {
                                    break 'inner;
                                };
                                match result {
                                    Ok(result) => {
                                        match process_blocks(result, authority_index) {
                                            Ok(blocks) => {
                                                let max_round = blocks.into_iter().map(|b|b.round()).max().unwrap_or(0);
                                                highest_round = highest_round.max(max_round);

                                                total_stake += context.committee.stake(authority_index);
                                            },
                                            Err(err) => {
                                                warn!("Invalid result returned from {authority_index} while fetching last own block: {err}");
                                            }
                                        }
                                    },
                                    Err(err) => {
                                        warn!("Error {err} while fetching our own block from peer {authority_index}. Will retry.");
                                        results.push(fetch_own_block(authority_index, FETCH_OWN_BLOCK_RETRY_DELAY));
                                    }
                                }
                            },
                            () = &mut timer => {
                                info!("Timeout while trying to sync our own last block from peers");
                                break 'inner;
                            }
                        }
                    }

                    // Request at least f+1 stake to have replied back.
                    if context.committee.reached_validity(total_stake) {
                        info!("{} out of {} total stake returned acceptable results for our own last block with highest round {}, with {retries} retries.", total_stake, context.committee.total_stake(), highest_round);
                        break 'main;
                    }

                    retries += 1;
                    context.metrics.node_metrics.sync_last_known_own_block_retries.inc();
                    warn!("Not enough stake: {} out of {} total stake returned acceptable results for our own last block with highest round {}. Will now retry {retries}.", total_stake, context.committee.total_stake(), highest_round);

                    sleep(retry_delay_step).await;

                    retry_delay_step = Duration::from_secs_f64(retry_delay_step.as_secs_f64() * 1.5);
                    retry_delay_step = retry_delay_step.min(MAX_RETRY_DELAY_STEP);
                }

                // Update the Core with the highest detected round
                context.metrics.node_metrics.last_known_own_block_round.set(highest_round as i64);

                if let Err(err) = core_dispatcher.set_last_known_proposed_round(highest_round) {
                    warn!("Error received while calling dispatcher, probably dispatcher is shutting down, will now exit: {err:?}");
                }
            }));
    }

    async fn start_fetch_missing_blocks_task(&mut self) -> ConsensusResult<()> {
        let mut missing_blocks = self
            .core_dispatcher
            .get_missing_blocks()
            .await
            .map_err(|_err| ConsensusError::Shutdown)?;

        // No reason to kick off the scheduler if there are no missing blocks to fetch
        if missing_blocks.is_empty() {
            return Ok(());
        }

        let context = self.context.clone();
        let network_client = self.network_client.clone();
        let block_verifier = self.block_verifier.clone();
        let commit_vote_monitor = self.commit_vote_monitor.clone();
        let core_dispatcher = self.core_dispatcher.clone();
        let blocks_to_fetch = self.inflight_blocks_map.clone();
        let commands_sender = self.commands_sender.clone();
        let dag_state = self.dag_state.clone();

        let (commit_lagging, last_commit_index, quorum_commit_index) = self.is_commit_lagging();
        if commit_lagging {
            // As node is commit lagging try to sync only the missing blocks that are within the acceptable round thresholds to sync. The rest we don't attempt to
            // sync yet.
            let highest_accepted_round = dag_state.read().highest_accepted_round();
            missing_blocks = missing_blocks
                .into_iter()
                .take_while(|b| {
                    b.round <= highest_accepted_round + SYNC_MISSING_BLOCK_ROUND_THRESHOLD
                })
                .collect::<BTreeSet<_>>();

            // If no missing blocks are within the acceptable thresholds to sync while we commit lag, then we disable the scheduler completely for this run.
            if missing_blocks.is_empty() {
                trace!("Scheduled synchronizer temporarily disabled as local commit is falling behind from quorum {last_commit_index} << {quorum_commit_index} and missing blocks are too far in the future.");
                self.context
                    .metrics
                    .node_metrics
                    .fetch_blocks_scheduler_skipped
                    .with_label_values(&["commit_lagging"])
                    .inc();
                return Ok(());
            }
        }

        self.fetch_blocks_scheduler_task
            .spawn(monitored_future!(async move {
                let _scope = monitored_scope("FetchMissingBlocksScheduler");
                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.inc();
                let total_requested = missing_blocks.len();

                fail_point_async!("consensus-delay");

                // Fetch blocks from peers
                let results = Self::fetch_blocks_from_authorities(context.clone(), blocks_to_fetch.clone(), network_client, missing_blocks, dag_state).await;
                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.dec();
                if results.is_empty() {
                    return;
                }

                // Now process the returned results
                let mut total_fetched = 0;
                for (blocks_guard, fetched_blocks, peer) in results {
                    total_fetched += fetched_blocks.len();

                    if let Err(err) = Self::process_fetched_blocks(fetched_blocks, peer, blocks_guard, core_dispatcher.clone(), block_verifier.clone(), commit_vote_monitor.clone(), context.clone(), commands_sender.clone(), "periodic").await {
                        warn!("Error occurred while processing fetched blocks from peer {peer}: {err}");
                    }
                }

                debug!("Total blocks requested to fetch: {}, total fetched: {}", total_requested, total_fetched);
            }));
        Ok(())
    }

    fn is_commit_lagging(&self) -> (bool, CommitIndex, CommitIndex) {
        let last_commit_index = self.dag_state.read().last_commit_index();
        let quorum_commit_index = self.commit_vote_monitor.quorum_commit_index();
        let commit_threshold = last_commit_index
            + self.context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER;
        (
            commit_threshold < quorum_commit_index,
            last_commit_index,
            quorum_commit_index,
        )
    }

    /// Fetches the `missing_blocks` from available peers. The method will attempt to split the load amongst multiple (random) peers.
    /// The method returns a vector with the fetched blocks from each peer that successfully responded and any corresponding additional ancestor blocks.
    /// Each element of the vector is a tuple which contains the requested missing block refs, the returned blocks and the peer authority index.
    async fn fetch_blocks_from_authorities(
        context: Arc<Context>,
        inflight_blocks: Arc<InflightBlocksMap>,
        network_client: Arc<C>,
        missing_blocks: BTreeSet<BlockRef>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Vec<(BlocksGuard, Vec<Bytes>, AuthorityIndex)> {
        const MAX_PEERS: usize = 3;

        // Attempt to fetch only up to a max of blocks
        let missing_blocks = missing_blocks
            .into_iter()
            .take(MAX_PEERS * MAX_BLOCKS_PER_FETCH)
            .collect::<Vec<_>>();

        let mut missing_blocks_per_authority = vec![0; context.committee.size()];
        for block in &missing_blocks {
            missing_blocks_per_authority[block.author] += 1;
        }
        for (missing, (_, authority)) in missing_blocks_per_authority
            .into_iter()
            .zip(context.committee.authorities())
        {
            context
                .metrics
                .node_metrics
                .synchronizer_missing_blocks_by_authority
                .with_label_values(&[&authority.hostname])
                .inc_by(missing as u64);
            context
                .metrics
                .node_metrics
                .synchronizer_current_missing_blocks_by_authority
                .with_label_values(&[&authority.hostname])
                .set(missing as i64);
        }

        let mut peers = context
            .committee
            .authorities()
            .filter_map(|(peer_index, _)| (peer_index != context.own_index).then_some(peer_index))
            .collect::<Vec<_>>();

        // TODO: probably inject the RNG to allow unit testing - this is a work around for now.
        if cfg!(not(test)) {
            // Shuffle the peers
            peers.shuffle(&mut ThreadRng::default());
        }

        let mut peers = peers.into_iter();
        let mut request_futures = FuturesUnordered::new();

        let highest_rounds = Self::get_highest_accepted_rounds(dag_state, &context);

        // Send the initial requests
        for blocks in missing_blocks.chunks(MAX_BLOCKS_PER_FETCH) {
            let peer = peers
                .next()
                .expect("Possible misconfiguration as a peer should be found");
            let peer_hostname = &context.committee.authority(peer).hostname;
            let block_refs = blocks.iter().cloned().collect::<BTreeSet<_>>();

            // lock the blocks to be fetched. If no lock can be acquired for any of the blocks then don't bother
            if let Some(blocks_guard) = inflight_blocks.lock_blocks(block_refs.clone(), peer) {
                info!(
                    "Periodic sync of {} missing blocks from peer {} {}: {}",
                    block_refs.len(),
                    peer,
                    peer_hostname,
                    block_refs
                        .iter()
                        .map(|b| b.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                request_futures.push(Self::fetch_blocks_request(
                    network_client.clone(),
                    peer,
                    blocks_guard,
                    highest_rounds.clone(),
                    FETCH_REQUEST_TIMEOUT,
                    1,
                ));
            }
        }

        let mut results = Vec::new();
        let fetcher_timeout = sleep(FETCH_FROM_PEERS_TIMEOUT);

        tokio::pin!(fetcher_timeout);

        loop {
            tokio::select! {
                Some((response, blocks_guard, _retries, peer_index, highest_rounds)) = request_futures.next() => {
                    let peer_hostname = &context.committee.authority(peer_index).hostname;
                    match response {
                        Ok(fetched_blocks) => {
                            results.push((blocks_guard, fetched_blocks, peer_index));

                            // no more pending requests are left, just break the loop
                            if request_futures.is_empty() {
                                break;
                            }
                        },
                        Err(_) => {
                            // try again if there is any peer left
                            if let Some(next_peer) = peers.next() {
                                // do best effort to lock guards. If we can't lock then don't bother at this run.
                                if let Some(blocks_guard) = inflight_blocks.swap_locks(blocks_guard, next_peer) {
                                    info!(
                                        "Retrying syncing {} missing blocks from peer {}: {}",
                                        blocks_guard.block_refs.len(),
                                        peer_hostname,
                                        blocks_guard.block_refs
                                            .iter()
                                            .map(|b| b.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    );
                                    request_futures.push(Self::fetch_blocks_request(
                                        network_client.clone(),
                                        next_peer,
                                        blocks_guard,
                                        highest_rounds,
                                        FETCH_REQUEST_TIMEOUT,
                                        1,
                                    ));
                                } else {
                                    debug!("Couldn't acquire locks to fetch blocks from peer {next_peer}.")
                                }
                            } else {
                                debug!("No more peers left to fetch blocks");
                            }
                        }
                    }
                },
                _ = &mut fetcher_timeout => {
                    debug!("Timed out while fetching missing blocks");
                    break;
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::{AuthorityIndex, Parameters};
    use parking_lot::RwLock;
    use tokio::{sync::Mutex, time::sleep};

    use crate::{
        authority_service::COMMIT_LAG_MULTIPLIER,
        core_thread::MockCoreThreadDispatcher,
        synchronizer::{MAX_BLOCKS_PER_FETCH, SYNC_MISSING_BLOCK_ROUND_THRESHOLD},
    };
    use crate::{
        block::{BlockDigest, BlockRef, Round, TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit::CommitRange,
        commit_vote_monitor::CommitVoteMonitor,
        context::Context,
        core_thread::CoreThreadDispatcher,
        dag_state::DagState,
        error::{ConsensusError, ConsensusResult},
        network::{BlockStream, NetworkClient},
        storage::mem_store::MemStore,
        synchronizer::{
            InflightBlocksMap, Synchronizer, FETCH_BLOCKS_CONCURRENCY, FETCH_REQUEST_TIMEOUT,
        },
        CommitDigest, CommitIndex,
    };
    use crate::{
        commit::{CommitVote, TrustedCommit},
        BlockAPI,
    };

    type FetchRequestKey = (Vec<BlockRef>, AuthorityIndex);
    type FetchRequestResponse = (Vec<VerifiedBlock>, Option<Duration>);
    type FetchLatestBlockKey = (AuthorityIndex, Vec<AuthorityIndex>);
    type FetchLatestBlockResponse = (Vec<VerifiedBlock>, Option<Duration>);

    #[derive(Default)]
    struct MockNetworkClient {
        fetch_blocks_requests: Mutex<BTreeMap<FetchRequestKey, FetchRequestResponse>>,
        fetch_latest_blocks_requests:
            Mutex<BTreeMap<FetchLatestBlockKey, Vec<FetchLatestBlockResponse>>>,
    }

    impl MockNetworkClient {
        async fn stub_fetch_blocks(
            &self,
            blocks: Vec<VerifiedBlock>,
            peer: AuthorityIndex,
            latency: Option<Duration>,
        ) {
            let mut lock = self.fetch_blocks_requests.lock().await;
            let block_refs = blocks
                .iter()
                .map(|block| block.reference())
                .collect::<Vec<_>>();
            lock.insert((block_refs, peer), (blocks, latency));
        }

        async fn stub_fetch_latest_blocks(
            &self,
            blocks: Vec<VerifiedBlock>,
            peer: AuthorityIndex,
            authorities: Vec<AuthorityIndex>,
            latency: Option<Duration>,
        ) {
            let mut lock = self.fetch_latest_blocks_requests.lock().await;
            lock.entry((peer, authorities))
                .or_default()
                .push((blocks, latency));
        }

        async fn fetch_latest_blocks_pending_calls(&self) -> usize {
            let lock = self.fetch_latest_blocks_requests.lock().await;
            lock.len()
        }
    }

    #[async_trait]
    impl NetworkClient for MockNetworkClient {
        const SUPPORT_STREAMING: bool = false;

        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }

        async fn subscribe_blocks(
            &self,
            _peer: AuthorityIndex,
            _last_received: Round,
            _timeout: Duration,
        ) -> ConsensusResult<BlockStream> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            let mut lock = self.fetch_blocks_requests.lock().await;
            let response = lock
                .remove(&(block_refs, peer))
                .expect("Unexpected fetch blocks request made");

            let serialised = response
                .0
                .into_iter()
                .map(|block| block.serialized().clone())
                .collect::<Vec<_>>();

            drop(lock);

            if let Some(latency) = response.1 {
                sleep(latency).await;
            }

            Ok(serialised)
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_latest_blocks(
            &self,
            peer: AuthorityIndex,
            authorities: Vec<AuthorityIndex>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            let mut lock = self.fetch_latest_blocks_requests.lock().await;
            let mut responses = lock
                .remove(&(peer, authorities.clone()))
                .expect("Unexpected fetch blocks request made");

            let response = responses.remove(0);
            let serialised = response
                .0
                .into_iter()
                .map(|block| block.serialized().clone())
                .collect::<Vec<_>>();

            if !responses.is_empty() {
                lock.insert((peer, authorities), responses);
            }

            drop(lock);

            if let Some(latency) = response.1 {
                sleep(latency).await;
            }

            Ok(serialised)
        }

        async fn get_latest_rounds(
            &self,
            _peer: AuthorityIndex,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Round>, Vec<Round>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[test]
    fn inflight_blocks_map() {
        // GIVEN
        let map = InflightBlocksMap::new();
        let some_block_refs = [
            BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(10, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(12, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
            BlockRef::new(15, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
        ];
        let missing_block_refs = some_block_refs.iter().cloned().collect::<BTreeSet<_>>();

        // Lock & unlock blocks
        {
            let mut all_guards = Vec::new();

            // Try to acquire the block locks for authorities 1 & 2
            for i in 1..=2 {
                let authority = AuthorityIndex::new_for_test(i);

                let guard = map.lock_blocks(missing_block_refs.clone(), authority);
                let guard = guard.expect("Guard should be created");
                assert_eq!(guard.block_refs.len(), 4);

                all_guards.push(guard);

                // trying to acquire any of them again will not succeed
                let guard = map.lock_blocks(missing_block_refs.clone(), authority);
                assert!(guard.is_none());
            }

            // Trying to acquire for authority 3 it will fail - as we have maxed out the number of allowed peers
            let authority_3 = AuthorityIndex::new_for_test(3);

            let guard = map.lock_blocks(missing_block_refs.clone(), authority_3);
            assert!(guard.is_none());

            // Explicitly drop the guard of authority 1 and try for authority 3 again - it will now succeed
            drop(all_guards.remove(0));

            let guard = map.lock_blocks(missing_block_refs.clone(), authority_3);
            let guard = guard.expect("Guard should be successfully acquired");

            assert_eq!(guard.block_refs, missing_block_refs);

            // Dropping all guards should unlock on the block refs
            drop(guard);
            drop(all_guards);

            assert_eq!(map.num_of_locked_blocks(), 0);
        }

        // Swap locks
        {
            // acquire a lock for authority 1
            let authority_1 = AuthorityIndex::new_for_test(1);
            let guard = map
                .lock_blocks(missing_block_refs.clone(), authority_1)
                .unwrap();

            // Now swap the locks for authority 2
            let authority_2 = AuthorityIndex::new_for_test(2);
            let guard = map.swap_locks(guard, authority_2);

            assert_eq!(guard.unwrap().block_refs, missing_block_refs);
        }
    }

    #[tokio::test]
    async fn successful_fetch_blocks_from_peer() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
            false,
        );

        // Create some test blocks
        let expected_blocks = (0..10)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()))
            .collect::<Vec<_>>();
        let missing_blocks = expected_blocks
            .iter()
            .map(|block| block.reference())
            .collect::<BTreeSet<_>>();

        // AND stub the fetch_blocks request from peer 1
        let peer = AuthorityIndex::new_for_test(1);
        network_client
            .stub_fetch_blocks(expected_blocks.clone(), peer, None)
            .await;

        // WHEN request missing blocks from peer 1
        assert!(handle.fetch_blocks(missing_blocks, peer).await.is_ok());

        // Wait a little bit until those have been added in core
        sleep(Duration::from_millis(1_000)).await;

        // THEN ensure those ended up in Core
        let added_blocks = core_dispatcher.get_add_blocks().await;
        assert_eq!(added_blocks, expected_blocks);
    }

    #[tokio::test]
    async fn saturate_fetch_blocks_from_peer() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
            false,
        );

        // Create some test blocks
        let expected_blocks = (0..=2 * FETCH_BLOCKS_CONCURRENCY)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round as Round, 0).build()))
            .collect::<Vec<_>>();

        // Now start sending requests to fetch blocks by trying to saturate peer 1 task
        let peer = AuthorityIndex::new_for_test(1);
        let mut iter = expected_blocks.iter().peekable();
        while let Some(block) = iter.next() {
            // stub the fetch_blocks request from peer 1 and give some high response latency so requests
            // can start blocking the peer task.
            network_client
                .stub_fetch_blocks(
                    vec![block.clone()],
                    peer,
                    Some(Duration::from_millis(5_000)),
                )
                .await;

            let mut missing_blocks = BTreeSet::new();
            missing_blocks.insert(block.reference());

            // WHEN requesting to fetch the blocks, it should not succeed for the last request and get
            // an error with "saturated" synchronizer
            if iter.peek().is_none() {
                match handle.fetch_blocks(missing_blocks, peer).await {
                    Err(ConsensusError::SynchronizerSaturated(index)) => {
                        assert_eq!(index, peer);
                    }
                    _ => panic!("A saturated synchronizer error was expected"),
                }
            } else {
                assert!(handle.fetch_blocks(missing_blocks, peer).await.is_ok());
            }
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn synchronizer_periodic_task_fetch_blocks() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

        // Create some test blocks
        let expected_blocks = (0..10)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()))
            .collect::<Vec<_>>();
        let missing_blocks = expected_blocks
            .iter()
            .map(|block| block.reference())
            .collect::<BTreeSet<_>>();

        // AND stub the missing blocks
        core_dispatcher
            .stub_missing_blocks(missing_blocks.clone())
            .await;

        // AND stub the requests for authority 1 & 2
        // Make the first authority timeout, so the second will be called. "We" are authority = 0, so
        // we are skipped anyways.
        network_client
            .stub_fetch_blocks(
                expected_blocks.clone(),
                AuthorityIndex::new_for_test(1),
                Some(FETCH_REQUEST_TIMEOUT),
            )
            .await;
        network_client
            .stub_fetch_blocks(
                expected_blocks.clone(),
                AuthorityIndex::new_for_test(2),
                None,
            )
            .await;

        // WHEN start the synchronizer and wait for a couple of seconds
        let _handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
            false,
        );

        sleep(2 * FETCH_REQUEST_TIMEOUT).await;

        // THEN the missing blocks should now be fetched and added to core
        let added_blocks = core_dispatcher.get_add_blocks().await;
        assert_eq!(added_blocks, expected_blocks);

        // AND missing blocks should have been consumed by the stub
        assert!(core_dispatcher
            .get_missing_blocks()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn synchronizer_periodic_task_when_commit_lagging_with_missing_blocks_in_acceptable_thresholds(
    ) {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        // AND stub some missing blocks. The highest accepted round is 0. Create some blocks that are below and above the threshold sync.
        let expected_blocks = (0..SYNC_MISSING_BLOCK_ROUND_THRESHOLD * 2)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()))
            .collect::<Vec<_>>();

        let missing_blocks = expected_blocks
            .iter()
            .map(|block| block.reference())
            .collect::<BTreeSet<_>>();
        core_dispatcher.stub_missing_blocks(missing_blocks).await;

        // AND stub the requests for authority 1 & 2
        // Make the first authority timeout, so the second will be called. "We" are authority = 0, so
        // we are skipped anyways.
        let mut expected_blocks = expected_blocks
            .into_iter()
            .filter(|block| block.round() <= SYNC_MISSING_BLOCK_ROUND_THRESHOLD)
            .collect::<Vec<_>>();

        for chunk in expected_blocks.chunks(MAX_BLOCKS_PER_FETCH) {
            network_client
                .stub_fetch_blocks(
                    chunk.to_vec(),
                    AuthorityIndex::new_for_test(1),
                    Some(FETCH_REQUEST_TIMEOUT),
                )
                .await;

            network_client
                .stub_fetch_blocks(chunk.to_vec(), AuthorityIndex::new_for_test(2), None)
                .await;
        }

        // Now create some blocks to simulate a commit lag
        let round = context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER * 2;
        let commit_index: CommitIndex = round - 1;
        let blocks = (0..4)
            .map(|authority| {
                let commit_votes = vec![CommitVote::new(commit_index, CommitDigest::MIN)];
                let block = TestBlock::new(round, authority)
                    .set_commit_votes(commit_votes)
                    .build();

                VerifiedBlock::new_for_test(block)
            })
            .collect::<Vec<_>>();

        // Pass them through the commit vote monitor - so now there will be a big commit lag to prevent
        // the scheduled synchronizer from running
        for block in blocks {
            commit_vote_monitor.observe_block(&block);
        }

        // WHEN start the synchronizer and wait for a couple of seconds where normally the synchronizer should have kicked in.
        let _handle = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            dag_state.clone(),
            false,
        );

        sleep(4 * FETCH_REQUEST_TIMEOUT).await;

        // We should be in commit lag mode, but since there are missing blocks within the acceptable round thresholds those ones should be fetched. Nothing above.
        let mut added_blocks = core_dispatcher.get_add_blocks().await;

        added_blocks.sort_by_key(|block| block.reference());
        expected_blocks.sort_by_key(|block| block.reference());

        assert_eq!(added_blocks, expected_blocks);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn synchronizer_periodic_task_when_commit_lagging_gets_disabled() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        // AND stub some missing blocks. The highest accepted round is 0. Create blocks that are above the threshold sync.
        let mut expected_blocks = (SYNC_MISSING_BLOCK_ROUND_THRESHOLD * 2
            ..SYNC_MISSING_BLOCK_ROUND_THRESHOLD * 3)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()))
            .collect::<Vec<_>>();
        let missing_blocks = expected_blocks
            .iter()
            .map(|block| block.reference())
            .collect::<BTreeSet<_>>();
        core_dispatcher
            .stub_missing_blocks(missing_blocks.clone())
            .await;

        // AND stub the requests for authority 1 & 2
        // Make the first authority timeout, so the second will be called. "We" are authority = 0, so
        // we are skipped anyways.
        for chunk in expected_blocks.chunks(MAX_BLOCKS_PER_FETCH) {
            network_client
                .stub_fetch_blocks(
                    chunk.to_vec(),
                    AuthorityIndex::new_for_test(1),
                    Some(FETCH_REQUEST_TIMEOUT),
                )
                .await;
            network_client
                .stub_fetch_blocks(chunk.to_vec(), AuthorityIndex::new_for_test(2), None)
                .await;
        }

        // Now create some blocks to simulate a commit lag
        let round = context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER * 2;
        let commit_index: CommitIndex = round - 1;
        let blocks = (0..4)
            .map(|authority| {
                let commit_votes = vec![CommitVote::new(commit_index, CommitDigest::MIN)];
                let block = TestBlock::new(round, authority)
                    .set_commit_votes(commit_votes)
                    .build();

                VerifiedBlock::new_for_test(block)
            })
            .collect::<Vec<_>>();

        // Pass them through the commit vote monitor - so now there will be a big commit lag to prevent
        // the scheduled synchronizer from running
        for block in blocks {
            commit_vote_monitor.observe_block(&block);
        }

        // WHEN start the synchronizer and wait for a couple of seconds where normally the synchronizer should have kicked in.
        let _handle = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier,
            dag_state.clone(),
            false,
        );

        sleep(4 * FETCH_REQUEST_TIMEOUT).await;

        // Since we should be in commit lag mode none of the missed blocks should have been fetched - hence nothing should be
        // sent to core for processing.
        let added_blocks = core_dispatcher.get_add_blocks().await;
        assert_eq!(added_blocks, vec![]);

        // AND advance now the local commit index by adding a new commit that matches the commit index
        // of quorum
        {
            let mut d = dag_state.write();
            for index in 1..=commit_index {
                let commit =
                    TrustedCommit::new_for_test(index, CommitDigest::MIN, 0, BlockRef::MIN, vec![]);

                d.add_commit(commit);
            }

            assert_eq!(
                d.last_commit_index(),
                commit_vote_monitor.quorum_commit_index()
            );
        }

        // Now stub again the missing blocks to fetch the exact same ones.
        core_dispatcher
            .stub_missing_blocks(missing_blocks.clone())
            .await;

        sleep(2 * FETCH_REQUEST_TIMEOUT).await;

        // THEN the missing blocks should now be fetched and added to core
        let mut added_blocks = core_dispatcher.get_add_blocks().await;

        added_blocks.sort_by_key(|block| block.reference());
        expected_blocks.sort_by_key(|block| block.reference());

        assert_eq!(added_blocks, expected_blocks);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn synchronizer_fetch_own_last_block() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context.with_parameters(Parameters {
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        }));
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let our_index = AuthorityIndex::new_for_test(0);

        // Create some test blocks
        let mut expected_blocks = (9..=10)
            .map(|round| VerifiedBlock::new_for_test(TestBlock::new(round, 0).build()))
            .collect::<Vec<_>>();

        // Now set different latest blocks for the peers
        // For peer 1 we give the block of round 10 (highest)
        let block_1 = expected_blocks.pop().unwrap();
        network_client
            .stub_fetch_latest_blocks(
                vec![block_1.clone()],
                AuthorityIndex::new_for_test(1),
                vec![our_index],
                None,
            )
            .await;
        network_client
            .stub_fetch_latest_blocks(
                vec![block_1],
                AuthorityIndex::new_for_test(1),
                vec![our_index],
                None,
            )
            .await;

        // For peer 2 we give the block of round 9
        let block_2 = expected_blocks.pop().unwrap();
        network_client
            .stub_fetch_latest_blocks(
                vec![block_2.clone()],
                AuthorityIndex::new_for_test(2),
                vec![our_index],
                Some(Duration::from_secs(10)),
            )
            .await;
        network_client
            .stub_fetch_latest_blocks(
                vec![block_2],
                AuthorityIndex::new_for_test(2),
                vec![our_index],
                None,
            )
            .await;

        // For peer 3 we don't give any block - and it should return an empty vector
        network_client
            .stub_fetch_latest_blocks(
                vec![],
                AuthorityIndex::new_for_test(3),
                vec![our_index],
                Some(Duration::from_secs(10)),
            )
            .await;
        network_client
            .stub_fetch_latest_blocks(
                vec![],
                AuthorityIndex::new_for_test(3),
                vec![our_index],
                None,
            )
            .await;

        // WHEN start the synchronizer and wait for a couple of seconds
        let handle = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
            true,
        );

        // Wait at least for the timeout time
        sleep(context.parameters.sync_last_known_own_block_timeout * 2).await;

        // Assert that core has been called to set the min propose round
        assert_eq!(
            core_dispatcher.get_last_own_proposed_round().await,
            vec![10]
        );

        // Ensure that all the requests have been called
        assert_eq!(network_client.fetch_latest_blocks_pending_calls().await, 0);

        // And we got one retry
        assert_eq!(
            context
                .metrics
                .node_metrics
                .sync_last_known_own_block_retries
                .get(),
            1
        );

        // Ensure that no panic occurred
        if let Err(err) = handle.stop().await {
            if err.is_panic() {
                std::panic::resume_unwind(err.into_panic());
            }
        }
    }
}
