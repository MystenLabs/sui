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
#[cfg(not(test))]
use rand::{prelude::SliceRandom, rngs::ThreadRng};
use sui_macros::fail_point_async;
use tokio::{
    sync::{mpsc::error::TrySendError, oneshot},
    task::JoinSet,
    time::{sleep, sleep_until, timeout, Instant},
};
use tracing::{debug, error, info, trace, warn};

use crate::authority_service::COMMIT_LAG_MULTIPLIER;
use crate::commit_syncer::CommitVoteMonitor;
use crate::{
    block::{BlockRef, SignedBlock, VerifiedBlock},
    block_verifier::BlockVerifier,
    context::Context,
    core_thread::CoreThreadDispatcher,
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    network::NetworkClient,
    BlockAPI, CommitIndex, Round,
};

/// The number of concurrent fetch blocks requests per authority
const FETCH_BLOCKS_CONCURRENCY: usize = 5;

const FETCH_REQUEST_TIMEOUT: Duration = Duration::from_millis(2_000);

const FETCH_FROM_PEERS_TIMEOUT: Duration = Duration::from_millis(4_000);

const MAX_AUTHORITIES_TO_FETCH_PER_BLOCK: usize = 2;

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
    KickOffScheduler,
}

pub(crate) struct SynchronizerHandle {
    commands_sender: Sender<Command>,
    tasks: Mutex<JoinSet<()>>,
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

    pub(crate) async fn stop(&self) {
        let mut tasks = self.tasks.lock();
        tasks.abort_all();
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
pub(crate) struct Synchronizer<C: NetworkClient, V: BlockVerifier, D: CoreThreadDispatcher> {
    context: Arc<Context>,
    commands_receiver: Receiver<Command>,
    fetch_block_senders: BTreeMap<AuthorityIndex, Sender<BlocksGuard>>,
    core_dispatcher: Arc<D>,
    commit_vote_monitor: Arc<CommitVoteMonitor>,
    dag_state: Arc<RwLock<DagState>>,
    fetch_blocks_scheduler_task: JoinSet<()>,
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
            let context_cloned = context.clone();
            let network_cloned = network_client.clone();
            let block_verified_cloned = block_verifier.clone();
            let core_thread_dispatcher_cloned = core_dispatcher.clone();
            let dag_state_cloned = dag_state.clone();
            let command_sender_cloned = commands_sender.clone();

            tasks.spawn(monitored_future!(Self::fetch_blocks_from_authority(
                index,
                network_cloned,
                block_verified_cloned,
                context_cloned,
                core_thread_dispatcher_cloned,
                dag_state_cloned,
                receiver,
                command_sender_cloned,
            )));
            fetch_block_senders.insert(index, sender);
        }

        let commands_sender_clone = commands_sender.clone();

        // Spawn the task to listen to the requests & periodic runs
        tasks.spawn(monitored_future!(async move {
            let mut s = Self {
                context,
                commands_receiver,
                fetch_block_senders,
                core_dispatcher,
                fetch_blocks_scheduler_task: JoinSet::new(),
                network_client,
                block_verifier,
                inflight_blocks_map,
                commands_sender: commands_sender_clone,
                dag_state,
                commit_vote_monitor,
            };
            s.run().await;
        }));

        Arc::new(SynchronizerHandle {
            commands_sender,
            tasks: Mutex::new(tasks),
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
                                .take(self.context.parameters.max_blocks_per_fetch)
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
                        },
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
        context: Arc<Context>,
        core_dispatcher: Arc<D>,
        dag_state: Arc<RwLock<DagState>>,
        mut receiver: Receiver<BlocksGuard>,
        commands_sender: Sender<Command>,
    ) {
        const MAX_RETRIES: u32 = 5;

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
                                context.clone(),
                                commands_sender.clone(),
                                "live"
                            ).await {
                                warn!("Error while processing fetched blocks from peer {peer_index}: {err}");
                            }
                        },
                        Err(_) => {
                            if retries <= MAX_RETRIES {
                                requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, blocks_guard, highest_rounds, FETCH_REQUEST_TIMEOUT, retries))
                            } else {
                                warn!("Max retries {retries} reached while trying to fetch blocks from peer {peer_index}.");
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
        let blocks = Self::verify_blocks(
            serialized_blocks,
            block_verifier.clone(),
            &context,
            peer_index,
        )?;

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

        let metrics = &context.metrics.node_metrics;
        let peer_hostname = &context.committee.authority(peer_index).hostname;
        metrics
            .synchronizer_fetched_blocks_by_peer
            .with_label_values(&[peer_hostname, &sync_method])
            .inc_by(blocks.len() as u64);
        for block in &blocks {
            let block_hostname = &context.committee.authority(block.author()).hostname;
            metrics
                .synchronizer_fetched_blocks_by_authority
                .with_label_values(&[block_hostname, &sync_method])
                .inc();
        }

        debug!(
            "Synced missing ancestor blocks {} from peer {peer_index}",
            blocks.iter().map(|b| b.reference().to_string()).join(","),
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
            .map(|block| block.round())
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

            // TODO: dedup block verifications, here and with fetched blocks.
            if let Err(e) = block_verifier.verify(&signed_block) {
                // TODO: we might want to use a different metric to track the invalid "served" blocks
                // from the invalid "proposed" ones.
                context
                    .metrics
                    .node_metrics
                    .invalid_blocks
                    .with_label_values(&[&signed_block.author().to_string(), "synchronizer"])
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
                    "Fetched block {} timestamp {} is in the future (now={}). Ignoring.",
                    verified_block,
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

    async fn start_fetch_missing_blocks_task(&mut self) -> ConsensusResult<()> {
        let (commit_lagging, last_commit_index, quorum_commit_index) = self.is_commit_lagging();
        if commit_lagging {
            trace!("Scheduled synchronizer temporarily disabled as local commit is falling behind from quorum {last_commit_index} << {quorum_commit_index}");
            self.context
                .metrics
                .node_metrics
                .fetch_blocks_scheduler_skipped
                .with_label_values(&["commit_lagging"])
                .inc();
            return Ok(());
        }

        let missing_blocks = self
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
        let core_dispatcher = self.core_dispatcher.clone();
        let blocks_to_fetch = self.inflight_blocks_map.clone();
        let commands_sender = self.commands_sender.clone();
        let dag_state = self.dag_state.clone();

        self.fetch_blocks_scheduler_task
            .spawn(monitored_future!(async move {
                let _scope = monitored_scope("FetchMissingBlocksScheduler");
                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.inc();
                let total_requested = missing_blocks.len();

                fail_point_async!("consensus-delay");

                // Fetch blocks from peers
                let results = Self::fetch_blocks_from_authorities(context.clone(), blocks_to_fetch.clone(), network_client, missing_blocks, core_dispatcher.clone(), dag_state).await;
                if results.is_empty() {
                    warn!("No results returned while requesting missing blocks");
                    return;
                }

                // Now process the returned results
                let mut total_fetched = 0;
                for (blocks_guard, fetched_blocks, peer) in results {
                    total_fetched += fetched_blocks.len();

                    if let Err(err) = Self::process_fetched_blocks(fetched_blocks, peer, blocks_guard, core_dispatcher.clone(), block_verifier.clone(), context.clone(), commands_sender.clone(), "periodic").await {
                        warn!("Error occurred while processing fetched blocks from peer {peer}: {err}");
                    }
                }

                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.dec();
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
        _core_dispatcher: Arc<D>,
        dag_state: Arc<RwLock<DagState>>,
    ) -> Vec<(BlocksGuard, Vec<Bytes>, AuthorityIndex)> {
        const MAX_PEERS: usize = 3;

        // Attempt to fetch only up to a max of blocks
        let missing_blocks = missing_blocks
            .into_iter()
            .take(MAX_PEERS * context.parameters.max_blocks_per_fetch)
            .collect::<Vec<_>>();

        #[allow(unused_mut)]
        let mut peers = context
            .committee
            .authorities()
            .filter_map(|(peer_index, _)| (peer_index != context.own_index).then_some(peer_index))
            .collect::<Vec<_>>();

        // TODO: probably inject the RNG to allow unit testing - this is a work around for now.
        cfg_if::cfg_if! {
            if #[cfg(not(test))] {
                // Shuffle the peers
                peers.shuffle(&mut ThreadRng::default());
            }
        }

        let mut peers = peers.into_iter();
        let mut request_futures = FuturesUnordered::new();

        let highest_rounds = Self::get_highest_accepted_rounds(dag_state, &context);

        // Send the initial requests
        for blocks in missing_blocks.chunks(context.parameters.max_blocks_per_fetch) {
            let peer = peers
                .next()
                .expect("Possible misconfiguration as a peer should be found");
            let block_refs = blocks.iter().cloned().collect::<BTreeSet<_>>();

            // lock the blocks to be fetched. If no lock can be acquired for any of the blocks then don't bother
            if let Some(blocks_guard) = inflight_blocks.lock_blocks(block_refs.clone(), peer) {
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
                Some((response, blocks_guard, _retries, peer_index, highest_rounds)) = request_futures.next() =>
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
                    },
                _ = &mut fetcher_timeout => {
                    debug!("Timed out while fetching all the blocks");
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
    use consensus_config::AuthorityIndex;
    use parking_lot::RwLock;
    use tokio::time::sleep;

    use crate::authority_service::COMMIT_LAG_MULTIPLIER;
    use crate::commit::{CommitVote, TrustedCommit};
    use crate::commit_syncer::CommitVoteMonitor;
    use crate::{
        block::{BlockDigest, BlockRef, Round, TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit::CommitRange,
        context::Context,
        core_thread::{CoreError, CoreThreadDispatcher},
        dag_state::DagState,
        error::{ConsensusError, ConsensusResult},
        network::{BlockStream, NetworkClient},
        storage::mem_store::MemStore,
        synchronizer::{
            InflightBlocksMap, Synchronizer, FETCH_BLOCKS_CONCURRENCY, FETCH_REQUEST_TIMEOUT,
        },
        CommitDigest, CommitIndex,
    };

    // TODO: create a complete Mock for thread dispatcher to be used from several tests
    #[derive(Default)]
    struct MockCoreThreadDispatcher {
        add_blocks: tokio::sync::Mutex<Vec<VerifiedBlock>>,
        missing_blocks: tokio::sync::Mutex<BTreeSet<BlockRef>>,
    }

    impl MockCoreThreadDispatcher {
        async fn get_add_blocks(&self) -> Vec<VerifiedBlock> {
            let mut lock = self.add_blocks.lock().await;
            lock.drain(0..).collect()
        }

        async fn stub_missing_blocks(&self, block_refs: BTreeSet<BlockRef>) {
            let mut lock = self.missing_blocks.lock().await;
            lock.extend(block_refs);
        }
    }

    #[async_trait]
    impl CoreThreadDispatcher for MockCoreThreadDispatcher {
        async fn add_blocks(
            &self,
            blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            let mut lock = self.add_blocks.lock().await;
            lock.extend(blocks);
            Ok(BTreeSet::new())
        }

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            todo!()
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            let mut lock = self.missing_blocks.lock().await;
            let result = lock.clone();
            lock.clear();
            Ok(result)
        }

        fn set_consumer_availability(&self, _available: bool) -> Result<(), CoreError> {
            todo!()
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            todo!()
        }
    }

    type FetchRequestKey = (Vec<BlockRef>, AuthorityIndex);
    type FetchRequestResponse = (Vec<VerifiedBlock>, Option<Duration>);

    #[derive(Default)]
    struct MockNetworkClient {
        fetch_blocks_requests: tokio::sync::Mutex<BTreeMap<FetchRequestKey, FetchRequestResponse>>,
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
            _peer: AuthorityIndex,
            _authorities: Vec<AuthorityIndex>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            todo!()
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
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
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
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            commit_vote_monitor,
            block_verifier,
            dag_state,
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
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

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
    async fn synchronizer_periodic_task_skip_when_commit_lagging() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let network_client = Arc::new(MockNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        // AND stub some missing blocks
        let expected_blocks = (0..10)
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
            commit_vote_monitor.observe(&block);
        }

        // WHEN start the synchronizer and wait for a couple of seconds where normally the synchronizer should have kicked in.
        let _handle = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier,
            dag_state.clone(),
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

        sleep(2 * FETCH_REQUEST_TIMEOUT).await;

        // THEN the missing blocks should now be fetched and added to core
        let added_blocks = core_dispatcher.get_add_blocks().await;
        assert_eq!(added_blocks, expected_blocks);
    }
}
