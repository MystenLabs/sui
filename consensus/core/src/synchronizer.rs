// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::{BlockRef, SignedBlock, VerifiedBlock};
use crate::block_verifier::BlockVerifier;
use crate::context::Context;
use crate::core_thread::CoreThreadDispatcher;
use crate::error::{ConsensusError, ConsensusResult};
use crate::network::NetworkClient;
use bytes::Bytes;
use consensus_config::AuthorityIndex;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use parking_lot::Mutex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::{error::Elapsed, sleep_until, timeout, Instant};
use tracing::{info, warn};

/// The number of concurrent fetch blocks requests per authority
const FETCH_BLOCKS_CONCURRENCY: usize = 5;

enum Command {
    FetchBlocks {
        missing_block_refs: BTreeSet<BlockRef>,
        peer_index: AuthorityIndex,
        result: oneshot::Sender<Result<(), ConsensusError>>,
    },
}

#[allow(dead_code)]
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
        let (sender, receiver) = oneshot::channel();
        self.commands_sender
            .send(Command::FetchBlocks {
                missing_block_refs: block_refs,
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

#[allow(dead_code)]
pub(crate) struct Synchronizer {
    context: Arc<Context>,
    commands_receiver: Receiver<Command>,
    fetch_block_senders: BTreeMap<AuthorityIndex, Sender<BTreeSet<BlockRef>>>,
}

impl Synchronizer {
    pub fn start<C: NetworkClient, V: BlockVerifier, D: CoreThreadDispatcher>(
        network_client: Arc<C>,
        context: Arc<Context>,
        core_dispatcher: Arc<D>,
        block_verifier: Arc<V>,
    ) -> Arc<SynchronizerHandle> {
        let (commands_sender, commands_receiver) = channel(1_000);

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
            ));
            fetch_block_senders.insert(index, sender);
        }

        // Spawn the task to listen to the requests & periodic runs
        tasks.spawn(async {
            let mut s = Self {
                context,
                commands_receiver,
                fetch_block_senders,
            };
            s.run().await;
        });

        Arc::new(SynchronizerHandle {
            commands_sender,
            tasks: Mutex::new(tasks),
        })
    }

    async fn fetch_blocks_from_authority<
        C: NetworkClient,
        V: BlockVerifier,
        D: CoreThreadDispatcher,
    >(
        peer_index: AuthorityIndex,
        network_client: Arc<C>,
        block_verifier: Arc<V>,
        context: Arc<Context>,
        core_dispatcher: Arc<D>,
        mut receiver: Receiver<BTreeSet<BlockRef>>,
    ) {
        const REQUEST_TIMEOUT: Duration = Duration::from_millis(2_000);
        const MAX_RETRIES: u32 = 5;

        let mut requests = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(block_refs) = receiver.recv(), if requests.len() < FETCH_BLOCKS_CONCURRENCY => {
                    requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, block_refs, REQUEST_TIMEOUT, 1))
                },
                Some((response, block_refs, retries)) = requests.next() => {
                    match response {
                        Ok(Ok(blocks)) => {
                            if let Err(err) = Self::process_fetched_blocks(blocks,
                                peer_index,
                                block_refs,
                                core_dispatcher.clone(),
                                block_verifier.clone(),
                                context.clone()).await {
                                warn!("Error while processing fetched blocks from peer {peer_index}: {err}");
                            }
                        },
                        Ok(Err(_)) | Err(Elapsed {..}) => {
                            if retries <= MAX_RETRIES {
                                requests.push(Self::fetch_blocks_request(network_client.clone(), peer_index, block_refs, REQUEST_TIMEOUT, retries))
                            } else {
                                warn!("Max retries {retries} reached while trying to fetch blocks from peer {peer_index}.");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Processes the requested raw fetched blocks from peer `peer_index`. If no error is returned then
    /// the verified blocks are immediately sent to Core for processing.
    async fn process_fetched_blocks<V: BlockVerifier, D: CoreThreadDispatcher>(
        serialized_blocks: Vec<Bytes>,
        peer_index: AuthorityIndex,
        requested_block_refs: BTreeSet<BlockRef>,
        core_dispatcher: Arc<D>,
        block_verifier: Arc<V>,
        context: Arc<Context>,
    ) -> ConsensusResult<()> {
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
                    .with_label_values(&[&peer_index.to_string(), "synchronizer"])
                    .inc();
                info!("Invalid block from {}: {}", peer_index, e);
                return Err(e);
            }
            let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);

            // we want the peer to only respond with blocks that we have asked for.
            if !requested_block_refs.contains(&verified_block.reference()) {
                return Err(ConsensusError::UnexpectedFetchedBlock {
                    index: peer_index,
                    block_ref: verified_block.reference(),
                });
            }

            verified_blocks.push(verified_block);
        }

        // Now send them to core for processing. Ignore the returned missing blocks as we don't want
        // this mechanism to keep feedback looping on fetching more blocks. The periodic synchronization
        // will take care of that.
        let _ = core_dispatcher
            .add_blocks(verified_blocks)
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        Ok(())
    }

    async fn fetch_blocks_request<C: NetworkClient>(
        network_client: Arc<C>,
        peer: AuthorityIndex,
        block_refs: BTreeSet<BlockRef>,
        request_timeout: Duration,
        mut retries: u32,
    ) -> (
        Result<ConsensusResult<Vec<Bytes>>, Elapsed>,
        BTreeSet<BlockRef>,
        u32,
    ) {
        let start = Instant::now();
        let resp = timeout(
            request_timeout,
            network_client.fetch_blocks(peer, block_refs.clone().into_iter().collect::<Vec<_>>()),
        )
        .await;

        if matches!(resp, Ok(Err(_))) {
            // Add a delay before retrying - if that is needed. If request has timed out then eventually
            // this will be a no-op.
            sleep_until(start + request_timeout).await;
            retries += 1;
        }
        (resp, block_refs, retries)
    }

    // The main loop to listen for the submitted commands.
    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(command) = self.commands_receiver.recv() => {
                    match command {
                        Command::FetchBlocks{ missing_block_refs, peer_index, result } => {
                            assert_ne!(peer_index, self.context.own_index, "We should never attempt to fetch blocks from our own node");

                            if missing_block_refs.is_empty() {
                                result.send(Ok(())).ok();
                                continue;
                            }

                            // We don't block if the corresponding peer task is saturated - but we rather drop the request. That's ok as the periodic
                            // synchronization task will handle any still missing blocks in next run.
                            let r = self.fetch_block_senders.get(&peer_index).expect("Fatal error, sender should be present").try_send(missing_block_refs).map_err(|err| {
                                match err {
                                    TrySendError::Full(_) => ConsensusError::SynchronizerSaturated(peer_index),
                                    TrySendError::Closed(_) => ConsensusError::Shutdown
                                }
                            });
                            result.send(r).ok();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::block::{BlockRef, Round, TestBlock, VerifiedBlock};
    use crate::block_verifier::NoopBlockVerifier;
    use crate::context::Context;
    use crate::core_thread::{CoreError, CoreThreadDispatcher};
    use crate::error::{ConsensusError, ConsensusResult};
    use crate::network::NetworkClient;
    use crate::synchronizer::{Synchronizer, FETCH_BLOCKS_CONCURRENCY};
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
    }

    impl MockCoreThreadDispatcher {
        async fn get_add_blocks(&self) -> Vec<VerifiedBlock> {
            let lock = self.add_blocks.lock().await;
            lock.to_vec()
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
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &Bytes,
        ) -> ConsensusResult<()> {
            todo!()
        }

        async fn fetch_blocks(
            &self,
            peer: AuthorityIndex,
            block_refs: Vec<BlockRef>,
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
}
