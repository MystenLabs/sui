// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mysten_metrics::{monitored_future, monitored_scope};
use parking_lot::Mutex;
#[cfg(not(test))]
use rand::{prelude::SliceRandom, rngs::ThreadRng};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::{sleep, sleep_until, timeout, Instant};
use tracing::{debug, info, warn};

use crate::block::{BlockRef, SignedBlock, VerifiedBlock};
use crate::block_verifier::BlockVerifier;
use crate::context::Context;
use crate::core_thread::CoreThreadDispatcher;
use crate::error::{ConsensusError, ConsensusResult};
use crate::network::NetworkClient;
use crate::{BlockAPI, Round};
use consensus_config::AuthorityIndex;

/// The number of concurrent fetch blocks requests per authority
const FETCH_BLOCKS_CONCURRENCY: usize = 5;

const FETCH_REQUEST_TIMEOUT: Duration = Duration::from_millis(2_000);

const FETCH_FROM_PEERS_TIMEOUT: Duration = Duration::from_millis(4_000);

const MAX_FETCH_BLOCKS_PER_REQUEST: usize = 200;

const MAX_AUTHORITIES_TO_FETCH_PER_BLOCK: usize = 2;

struct BlocksGuard {
    map: Arc<InflightBlocksMap>,
    block_refs: BTreeSet<BlockRef>,
    peer: AuthorityIndex,
}

impl Drop for BlocksGuard {
    fn drop(&mut self) {
        self.map.unlock_blocks(self.block_refs.clone(), self.peer)
    }
}

// Keeps a mapping between the missing blocks that have been instructed to be fectched and the authorities
// that are currently fetching them.
struct InflightBlocksMap {
    inner: Mutex<HashMap<BlockRef, BTreeMap<AuthorityIndex, u32>>>,
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
    /// If `skip_checks = true` then no checks are made against the max authorities to fetch per block or
    /// same peer already fetching the same block. This is currently used only from the scheduler to
    /// allow us attempt fetching the blocks concurrently.
    fn lock_blocks(
        self: &Arc<Self>,
        context: &Context,
        missing_block_refs: BTreeSet<BlockRef>,
        peer: AuthorityIndex,
        skip_checks: bool,
    ) -> Option<BlocksGuard> {
        let hostname = context.committee.authority(peer).hostname.clone();

        let mut blocks = BTreeSet::new();
        let mut inner = self.inner.lock();

        for block_ref in missing_block_refs {
            // check that the number of authorities that are already instructed to fetch the block is not
            // higher than the allowed and the `peer_index` has not already been instructed to do that.
            let authorities = inner.entry(block_ref).or_default();
            if skip_checks
                || (authorities.len() < MAX_AUTHORITIES_TO_FETCH_PER_BLOCK
                    && authorities.get(&peer).is_none())
            {
                authorities.entry(peer).and_modify(|c| *c += 1).or_insert(1);
                blocks.insert(block_ref);
                context
                    .metrics
                    .node_metrics
                    .fetched_blocks_additional_authority
                    .with_label_values(&[&hostname])
                    .inc();
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

    /// Unlocks the provided block references for the given `peer`.
    fn unlock_blocks(self: &Arc<Self>, block_refs: BTreeSet<BlockRef>, peer: AuthorityIndex) {
        // Now mark all the blocks as fetched from the map
        let mut blocks_to_fetch = self.inner.lock();
        for block_ref in &block_refs {
            let authorities = blocks_to_fetch
                .get_mut(block_ref)
                .expect("Should have found a non empty map");
            let count = authorities
                .get_mut(&peer)
                .expect("Should have found a peer entry");
            assert!(*count > 0, "Counter for active peers can not be zero");

            *count = count.saturating_sub(1);

            // if there is none pending anymore to fetch from this peer, then just remove completely the peer map
            if *count == 0 {
                authorities.remove(&peer);
            }

            // if the last one then just clean up
            if authorities.is_empty() {
                blocks_to_fetch.remove(block_ref);
            }
        }
    }

    /// Drops the provided `blocks_guard` which will force to unlock the blocks, and lock now again the
    /// referenced block refs. This method will acquire the new locks by skipping any checks.
    fn swap_locks(
        self: &Arc<Self>,
        context: &Context,
        blocks_guard: BlocksGuard,
        peer: AuthorityIndex,
    ) -> BlocksGuard {
        let block_refs = blocks_guard.block_refs.clone();

        // Explicitly drop the guard
        drop(blocks_guard);

        // Now create new guard
        self.lock_blocks(context, block_refs, peer, true)
            .expect("Guard should have been created successfully")
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
        block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
    ) -> ConsensusResult<()> {
        // Keep only the max allowed blocks to request. It is ok to reduce here as the scheduler
        // task will take care syncing whatever is leftover.
        let missing_block_refs = block_refs
            .into_iter()
            .take(MAX_FETCH_BLOCKS_PER_REQUEST)
            .collect();

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

pub(crate) struct Synchronizer<C: NetworkClient, V: BlockVerifier, D: CoreThreadDispatcher> {
    context: Arc<Context>,
    commands_receiver: Receiver<Command>,
    fetch_block_senders: BTreeMap<AuthorityIndex, Sender<BlocksGuard>>,
    core_dispatcher: Arc<D>,
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
        block_verifier: Arc<V>,
    ) -> Arc<SynchronizerHandle> {
        let (commands_sender, commands_receiver) = channel(1_000);
        let inflight_blocks_map = InflightBlocksMap::new();

        // Spawn the tasks to fetch the blocks from the others
        let mut fetch_block_senders = BTreeMap::new();
        let mut tasks = JoinSet::new();
        for (index, _) in context.committee.authorities() {
            if index == context.own_index {
                continue;
            }
            let (sender, receiver) = channel(FETCH_BLOCKS_CONCURRENCY);
            tasks.spawn(Self::fetch_blocks_from_authority(
                index,
                network_client.clone(),
                block_verifier.clone(),
                context.clone(),
                core_dispatcher.clone(),
                receiver,
                commands_sender.clone(),
            ));
            fetch_block_senders.insert(index, sender);
        }

        let commands_sender_clone = commands_sender.clone();

        // Spawn the task to listen to the requests & periodic runs
        tasks.spawn(async {
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
            };
            s.run().await;
        });

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
                            assert_ne!(peer_index, self.context.own_index, "We should never attempt to fetch blocks from our own node");

                            let blocks_guard = self.inflight_blocks_map.lock_blocks(&self.context, missing_block_refs, peer_index, false);
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
        mut receiver: Receiver<BlocksGuard>,
        commands_sender: Sender<Command>,
    ) {
        const MAX_RETRIES: u32 = 5;

        let mut requests = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(blocks_guard) = receiver.recv(), if requests.len() < FETCH_BLOCKS_CONCURRENCY => {

                    // get the highest accepted rounds
                    let highest_rounds = match core_dispatcher.get_highest_accepted_rounds().await {
                        Ok(rounds) => rounds,
                        Err(err) => {
                            debug!("Core is shutting down, synchronizer is shutting down: {err:?}");
                            return;
                        }
                    };

                    requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, blocks_guard, highest_rounds, FETCH_REQUEST_TIMEOUT, 1))
                },
                Some((response, blocks_guard, retries, _peer, highest_rounds)) = requests.next() => {
                    match response {
                        Ok((blocks, ancestor_blocks)) => {
                            context
                            .metrics
                            .node_metrics
                            .fetched_blocks.with_label_values(&[&peer_index.to_string(), "live"]).inc_by(blocks.len() as u64);

                            if let Err(err) = Self::process_fetched_blocks(blocks,
                                ancestor_blocks,
                                peer_index,
                                blocks_guard,
                                core_dispatcher.clone(),
                                block_verifier.clone(),
                                context.clone(),
                                commands_sender.clone()).await {
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
        serialized_ancestor_blocks: Vec<Bytes>,
        peer_index: AuthorityIndex,
        requested_blocks_guard: BlocksGuard,
        core_dispatcher: Arc<D>,
        block_verifier: Arc<V>,
        context: Arc<Context>,
        commands_sender: Sender<Command>,
    ) -> ConsensusResult<()> {
        // The maximum number of blocks that can be additionally fetched from the one requested - those
        // are potentially missing ancestors.
        const MAX_ADDITIONAL_BLOCKS: usize = 10;

        if serialized_blocks.len() > requested_blocks_guard.block_refs.len()
            || serialized_ancestor_blocks.len() > MAX_ADDITIONAL_BLOCKS
        {
            return Err(ConsensusError::TooManyFetchedBlocksReturned(peer_index));
        }

        // Verify the requested blocks
        let mut blocks = Self::verify_blocks(
            serialized_blocks,
            requested_blocks_guard.block_refs.clone(),
            block_verifier.clone(),
            context.clone(),
            peer_index,
        )?;

        // Now verify any additional ancestor blocks. We need to verify that the ancestor blocks are indeed ancestors of the requested blocks
        let ancestors = blocks
            .iter()
            .flat_map(|b| b.ancestors().to_vec())
            .collect::<BTreeSet<BlockRef>>();
        let ancestor_blocks = Self::verify_blocks(
            serialized_ancestor_blocks,
            ancestors,
            block_verifier.clone(),
            context.clone(),
            peer_index,
        )?;

        blocks.extend(ancestor_blocks);

        // now release all the locked blocks as they have been fetched and verified
        drop(requested_blocks_guard);

        // Now send them to core for processing. Ignore the returned missing blocks as we don't want
        // this mechanism to keep feedback looping on fetching more blocks. The periodic synchronization
        // will take care of that.
        let missing_blocks = core_dispatcher
            .add_blocks(blocks)
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

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

    fn verify_blocks(
        serialized_blocks: Vec<Bytes>,
        valid_block_refs: BTreeSet<BlockRef>,
        block_verifier: Arc<V>,
        context: Arc<Context>,
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

            if !valid_block_refs.contains(&verified_block.reference()) {
                return Err(ConsensusError::UnexpectedFetchedBlock {
                    index: peer_index,
                    block_ref: verified_block.reference(),
                });
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
        ConsensusResult<(Vec<Bytes>, Vec<Bytes>)>,
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
                Err(ConsensusError::NetworkRequestTimeout(err.to_string()))
            }
            Ok(result) => result,
        };
        (resp, blocks_guard, retries, peer, highest_rounds)
    }

    async fn start_fetch_missing_blocks_task(&mut self) -> ConsensusResult<()> {
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

        self.fetch_blocks_scheduler_task
            .spawn(monitored_future!(async move {
                let _scope = monitored_scope("FetchMissingBlocksScheduler");
                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.inc();
                let total_requested = missing_blocks.len();

                // Fetch blocks from peers
                let results = Self::fetch_blocks_from_authorities(context.clone(), blocks_to_fetch.clone(), network_client, missing_blocks, core_dispatcher.clone()).await;
                if results.is_empty() {
                    warn!("No results returned while requesting missing blocks");
                    return;
                }

                // Now process the returned results
                let mut total_fetched = 0;
                for (blocks_guard, fetched_blocks, ancestor_blocks, peer) in results {
                    total_fetched += fetched_blocks.len();
                    context.metrics.node_metrics.fetched_blocks.with_label_values(&[&peer.to_string(), "periodic"]).inc_by(fetched_blocks.len() as u64);

                    if let Err(err) = Self::process_fetched_blocks(fetched_blocks, ancestor_blocks, peer, blocks_guard, core_dispatcher.clone(), block_verifier.clone(), context.clone(), commands_sender.clone()).await {
                        warn!("Error occurred while processing fetched blocks from peer {peer}: {err}");
                    }
                }

                context.metrics.node_metrics.fetch_blocks_scheduler_inflight.dec();
                debug!("Total blocks requested to fetch: {}, total fetched: {}", total_requested, total_fetched);
            }));
        Ok(())
    }

    /// Fetches the `missing_blocks` from available peers. The method will attempt to split the load amongst multiple (random) peers.
    /// The method returns a vector with the fetched blocks from each peer that successfully responded and any corresponding additional ancestor blocks.
    /// Each element of the vector is a tuple which contains the requested missing block refs, the returned blocks and the peer authority index.
    async fn fetch_blocks_from_authorities(
        context: Arc<Context>,
        inflight_blocks: Arc<InflightBlocksMap>,
        network_client: Arc<C>,
        missing_blocks: BTreeSet<BlockRef>,
        core_dispatcher: Arc<D>,
    ) -> Vec<(BlocksGuard, Vec<Bytes>, Vec<Bytes>, AuthorityIndex)> {
        const MAX_PEERS: usize = 3;
        const MAX_TOTAL_BLOCKS_TO_FETCH: usize = MAX_PEERS * MAX_FETCH_BLOCKS_PER_REQUEST;

        // Attempt to fetch only up to a max of blocks
        let missing_blocks = missing_blocks
            .into_iter()
            .take(MAX_TOTAL_BLOCKS_TO_FETCH)
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

        // get the highest accepted rounds
        let highest_rounds = match core_dispatcher.get_highest_accepted_rounds().await {
            Ok(rounds) => rounds,
            Err(err) => {
                debug!("Core is shutting down, synchronizer is shutting down: {err:?}");
                return vec![];
            }
        };

        // Send the initial requests
        for blocks in missing_blocks.chunks(MAX_FETCH_BLOCKS_PER_REQUEST) {
            let peer = peers
                .next()
                .expect("Possible misconfiguration as a peer should be found");
            let block_refs = blocks.iter().cloned().collect::<BTreeSet<_>>();

            // lock the blocks to be fetched. Allow re-fetching from same peer
            let blocks_guard = inflight_blocks
                .lock_blocks(&context, block_refs.clone(), peer, true)
                .expect("We should always succeed getting lock guards");
            request_futures.push(Self::fetch_blocks_request(
                network_client.clone(),
                peer,
                blocks_guard,
                highest_rounds.clone(),
                FETCH_REQUEST_TIMEOUT,
                1,
            ));
        }

        let mut results = Vec::new();
        let fetcher_timeout = sleep(FETCH_FROM_PEERS_TIMEOUT);

        tokio::pin!(fetcher_timeout);

        loop {
            tokio::select! {
                Some((response, blocks_guard, _retries, peer_index, highest_rounds)) = request_futures.next() =>
                    match response {
                        Ok((fetched_blocks, ancestor_blocks)) => {
                            results.push((blocks_guard, fetched_blocks, ancestor_blocks, peer_index));

                            // no more pending requests are left, just break the loop
                            if request_futures.is_empty() {
                                break;
                            }
                        },
                        Err(_) => {
                            // try again if there is any peer left
                            if let Some(next_peer) = peers.next() {
                                let blocks_guard = inflight_blocks.swap_locks(&context, blocks_guard, next_peer);

                                request_futures.push(Self::fetch_blocks_request(
                                    network_client.clone(),
                                    next_peer,
                                    blocks_guard,
                                    highest_rounds,
                                    FETCH_REQUEST_TIMEOUT,
                                    1,
                                ));
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
    use crate::block::{BlockDigest, BlockRef, Round, TestBlock, VerifiedBlock};
    use crate::block_verifier::NoopBlockVerifier;
    use crate::context::Context;
    use crate::core_thread::{CoreError, CoreThreadDispatcher};
    use crate::error::{ConsensusError, ConsensusResult};
    use crate::network::NetworkClient;
    use crate::synchronizer::{
        InflightBlocksMap, Synchronizer, FETCH_BLOCKS_CONCURRENCY, FETCH_REQUEST_TIMEOUT,
    };
    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::AuthorityIndex;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    // TODO: create a complete Mock for thread dispatcher to be used from several tests
    #[derive(Default)]
    struct MockCoreThreadDispatcher {
        add_blocks: tokio::sync::Mutex<Vec<VerifiedBlock>>,
        missing_blocks: tokio::sync::Mutex<BTreeSet<BlockRef>>,
    }

    impl MockCoreThreadDispatcher {
        async fn get_add_blocks(&self) -> Vec<VerifiedBlock> {
            let lock = self.add_blocks.lock().await;
            lock.to_vec()
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

        async fn force_new_block(&self, _round: Round) -> Result<(), CoreError> {
            todo!()
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            let mut lock = self.missing_blocks.lock().await;
            let result = lock.clone();
            lock.clear();
            Ok(result)
        }

        async fn get_highest_accepted_rounds(&self) -> Result<Vec<Round>, CoreError> {
            Ok(vec![])
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
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &VerifiedBlock,
            _timeout: Duration,
        ) -> ConsensusResult<()> {
            todo!()
        }

        async fn fetch_blocks(
            &self,
            peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
            _highest_accepted_rounds: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
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

            Ok((serialised, vec![]))
        }
    }

    #[test]
    fn inflight_blocks_map() {
        // GIVEN
        let (context, _) = Context::new_for_test(4);
        let context = Arc::new(context);

        let map = InflightBlocksMap::new();
        let some_block_refs = [
            BlockRef::new(1, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(10, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            BlockRef::new(12, AuthorityIndex::new_for_test(3), BlockDigest::MIN),
            BlockRef::new(15, AuthorityIndex::new_for_test(2), BlockDigest::MIN),
        ];
        let missing_block_refs = some_block_refs.iter().cloned().collect::<BTreeSet<_>>();

        // Do not skip checks
        {
            let mut all_guards = Vec::new();

            // Try to acquire the block locks for authorities 1 & 2
            for i in 1..=2 {
                let authority = AuthorityIndex::new_for_test(i);

                let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority, false);
                let guard = guard.expect("Guard should be created");
                assert_eq!(guard.block_refs.len(), 4);

                all_guards.push(guard);

                // trying to acquire any of them again will not succeed
                let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority, false);
                assert!(guard.is_none());
            }

            // Trying to acquire for authority 3 it will fail - as we have maxed out the number of allowed peers
            let authority_3 = AuthorityIndex::new_for_test(3);

            let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority_3, false);
            assert!(guard.is_none());

            // Explicitly drop the guard of authority 1 and try for authority 3 again - it will now succeed
            drop(all_guards.remove(0));

            let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority_3, false);
            let guard = guard.expect("Guard should be successfully acquired");

            assert_eq!(guard.block_refs, missing_block_refs);

            // Dropping all guards should unlock on the block refs
            drop(guard);
            drop(all_guards);

            assert_eq!(map.num_of_locked_blocks(), 0);
        }

        // Skip checks
        {
            let mut all_guards = Vec::new();
            let authority = AuthorityIndex::new_for_test(1);

            // Try to acquire the block locks for authority 1 multiple times
            for _i in 0..5 {
                let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority, true);
                let guard = guard.expect("Guard should be created");
                assert_eq!(guard.block_refs.len(), 4);

                all_guards.push(guard);
            }

            // Now try to acquire locks without skipping checks
            let guard = map.lock_blocks(&context, missing_block_refs.clone(), authority, false);
            assert!(guard.is_none());

            drop(all_guards);

            assert_eq!(map.num_of_locked_blocks(), 0);
        }

        // Swap locks
        {
            // acquire a lock for authority 1
            let authority_1 = AuthorityIndex::new_for_test(1);
            let guard = map
                .lock_blocks(&context, missing_block_refs.clone(), authority_1, false)
                .unwrap();

            // Now swap the locks for authority 2
            let authority_2 = AuthorityIndex::new_for_test(2);
            let guard = map.swap_locks(&context, guard, authority_2);

            assert_eq!(guard.block_refs, missing_block_refs);
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

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            block_verifier,
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

        let handle = Synchronizer::start(
            network_client.clone(),
            context,
            core_dispatcher.clone(),
            block_verifier,
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
            block_verifier,
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
}
