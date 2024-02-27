// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration, time::Instant, vec};

use async_trait::async_trait;
use bytes::Bytes;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use parking_lot::RwLock;
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{
    block::{timestamp_utc_ms, BlockAPI, BlockRef, SignedBlock, VerifiedBlock},
    block_manager::BlockManager,
    block_verifier::{BlockVerifier, SignedBlockVerifier},
    broadcaster::Broadcaster,
    commit_observer::CommitObserver,
    context::Context,
    core::{Core, CoreSignals},
    core_thread::{ChannelCoreThreadDispatcher, CoreThreadDispatcher, CoreThreadHandle},
    dag_state::DagState,
    error::{ConsensusError, ConsensusResult},
    leader_timeout::{LeaderTimeoutTask, LeaderTimeoutTaskHandle},
    metrics::initialise_metrics,
    network::{anemo_network::AnemoManager, NetworkManager, NetworkService},
    storage::rocksdb_store::RocksDBStore,
    synchronizer::{Synchronizer, SynchronizerHandle},
    transaction::{TransactionClient, TransactionConsumer, TransactionVerifier},
    CommitConsumer,
};

// This type is used by Sui as part of starting consensus via MysticetiManager.
// It hides the details of the types.
pub struct ConsensusAuthority(AuthorityNode<AnemoManager>);

impl ConsensusAuthority {
    pub async fn start(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        protocol_keypair: ProtocolKeyPair,
        network_keypair: NetworkKeyPair,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        commit_consumer: CommitConsumer,
        registry: Registry,
    ) -> Self {
        let authority_node = AuthorityNode::start(
            own_index,
            committee,
            parameters,
            protocol_config,
            protocol_keypair,
            network_keypair,
            transaction_verifier,
            commit_consumer,
            registry,
        )
        .await;
        Self(authority_node)
    }

    pub async fn stop(self) {
        self.0.stop().await;
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        self.0.transaction_client()
    }
}

pub(crate) struct AuthorityNode<N>
where
    N: NetworkManager<AuthorityService<ChannelCoreThreadDispatcher>>,
{
    context: Arc<Context>,
    start_time: Instant,
    transaction_client: Arc<TransactionClient>,
    synchronizer: Arc<SynchronizerHandle>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
    core_thread_handle: CoreThreadHandle,
    broadcaster: Broadcaster,
    network_manager: N,
}

impl<N> AuthorityNode<N>
where
    N: NetworkManager<AuthorityService<ChannelCoreThreadDispatcher>>,
{
    pub(crate) async fn start(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        // To avoid accidentally leaking the private key, the protocol key pair should only be
        // kept in Core.
        protocol_keypair: ProtocolKeyPair,
        network_keypair: NetworkKeyPair,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        commit_consumer: CommitConsumer,
        registry: Registry,
    ) -> Self {
        info!(
            "Starting authority {}\n{:#?}\n{:#?}\n{:?}",
            own_index, committee, parameters, protocol_config.version
        );
        let context = Arc::new(Context::new(
            own_index,
            committee,
            parameters,
            protocol_config,
            initialise_metrics(registry),
        ));
        let start_time = Instant::now();

        // Create the transactions client and the transactions consumer
        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // Construct Core components.
        let (core_signals, signals_receivers) = CoreSignals::new();
        let store = Arc::new(RocksDBStore::new(&context.parameters.db_path_str_unsafe()));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let block_manager = BlockManager::new(context.clone(), dag_state.clone());
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer.sender,
            commit_consumer.last_processed_index,
            dag_state.clone(),
            store.clone(),
        );

        let core = Core::new(
            context.clone(),
            tx_consumer,
            block_manager,
            commit_observer,
            core_signals,
            protocol_keypair,
            dag_state.clone(),
            store,
        );

        let (core_dispatcher, core_thread_handle) =
            ChannelCoreThreadDispatcher::start(core, context.clone());
        let core_dispatcher = Arc::new(core_dispatcher);
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher.clone(), &signals_receivers, context.clone());

        // Create network manager and client.
        let network_manager = N::new(context.clone());
        let network_client = network_manager.client();

        // Create Broadcaster.
        let broadcaster =
            Broadcaster::new(context.clone(), network_client.clone(), &signals_receivers);

        // Start network service.
        let block_verifier = Arc::new(SignedBlockVerifier::new(
            context.clone(),
            transaction_verifier,
        ));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            block_verifier.clone(),
        );
        let network_service = Arc::new(AuthorityService {
            context: context.clone(),
            block_verifier,
            core_dispatcher,
            synchronizer: synchronizer.clone(),
            dag_state,
        });
        network_manager.install_service(network_keypair, network_service);

        Self {
            context,
            start_time,
            transaction_client: Arc::new(tx_client),
            synchronizer,
            leader_timeout_handle,
            core_thread_handle,
            broadcaster,
            network_manager,
        }
    }

    pub(crate) async fn stop(mut self) {
        info!(
            "Stopping authority. Total run time: {:?}",
            self.start_time.elapsed()
        );

        self.network_manager.stop().await;
        self.broadcaster.stop();
        self.core_thread_handle.stop();
        self.leader_timeout_handle.stop().await;
        self.synchronizer.stop().await;

        self.context
            .metrics
            .node_metrics
            .uptime
            .observe(self.start_time.elapsed().as_secs_f64());
    }

    pub(crate) fn transaction_client(&self) -> Arc<TransactionClient> {
        self.transaction_client.clone()
    }
}

/// Authority's network interface.
pub(crate) struct AuthorityService<C: CoreThreadDispatcher> {
    context: Arc<Context>,
    block_verifier: Arc<dyn BlockVerifier>,
    core_dispatcher: Arc<C>,
    synchronizer: Arc<SynchronizerHandle>,
    dag_state: Arc<RwLock<DagState>>,
}

#[async_trait]
impl<C: CoreThreadDispatcher> NetworkService for AuthorityService<C> {
    async fn handle_send_block(
        &self,
        peer: AuthorityIndex,
        serialized_block: Bytes,
    ) -> ConsensusResult<()> {
        // TODO: dedup block verifications, here and with fetched blocks.
        let signed_block: SignedBlock =
            bcs::from_bytes(&serialized_block).map_err(ConsensusError::MalformedBlock)?;

        // Reject blocks not produced by the peer.
        if peer != signed_block.author() {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string(), "send_block"])
                .inc();
            let e = ConsensusError::UnexpectedAuthority(signed_block.author(), peer);
            info!("Block with wrong authority from {}: {}", peer, e);
            return Err(e);
        }

        // Reject blocks failing validations.
        if let Err(e) = self.block_verifier.verify(&signed_block) {
            self.context
                .metrics
                .node_metrics
                .invalid_blocks
                .with_label_values(&[&peer.to_string(), "send_block"])
                .inc();
            info!("Invalid block from {}: {}", peer, e);
            return Err(e);
        }
        let verified_block = VerifiedBlock::new_verified(signed_block, serialized_block);

        // Reject block with timestamp too far in the future.
        let forward_time_drift = Duration::from_millis(
            verified_block
                .timestamp_ms()
                .saturating_sub(timestamp_utc_ms()),
        );
        if forward_time_drift > self.context.parameters.max_forward_time_drift {
            return Err(ConsensusError::BlockTooFarInFuture {
                block_timestamp: verified_block.timestamp_ms(),
                forward_time_drift,
            });
        }

        // Wait until the block's timestamp is current.
        if forward_time_drift > Duration::ZERO {
            self.context
                .metrics
                .node_metrics
                .block_timestamp_drift_wait_ms
                .with_label_values(&[&peer.to_string()])
                .inc_by(forward_time_drift.as_millis() as u64);
            sleep(forward_time_drift).await;
        }

        let missing_ancestors = self
            .core_dispatcher
            .add_blocks(vec![verified_block])
            .await
            .map_err(|_| ConsensusError::Shutdown)?;

        if !missing_ancestors.is_empty() {
            // schedule the fetching of them from this peer
            if let Err(err) = self
                .synchronizer
                .fetch_blocks(missing_ancestors, peer)
                .await
            {
                warn!("Errored while trying to fetch missing ancestors via synchronizer: {err}");
            }
        }

        Ok(())
    }

    async fn handle_fetch_blocks(
        &self,
        peer: AuthorityIndex,
        block_refs: Vec<BlockRef>,
    ) -> ConsensusResult<Vec<Bytes>> {
        const MAX_ALLOWED_FETCH_BLOCKS: usize = 200;

        if block_refs.len() > MAX_ALLOWED_FETCH_BLOCKS {
            return Err(ConsensusError::TooManyFetchBlocksRequested(peer));
        }

        // Some quick validation of the requested block refs
        for block in &block_refs {
            if !self.context.committee.is_valid_index(block.author) {
                return Err(ConsensusError::InvalidAuthorityIndex {
                    index: block.author,
                    max: self.context.committee.size(),
                });
            }
            if block.round == 0 {
                return Err(ConsensusError::UnexpectedGenesisBlockRequested);
            }
        }

        // For now ask dag state directly
        let blocks = self.dag_state.read().get_blocks(block_refs)?;

        // Return the serialised blocks
        let result = blocks
            .into_iter()
            .flatten()
            .map(|block| block.serialized().clone())
            .collect::<Vec<_>>();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use async_trait::async_trait;
    use consensus_config::{local_committee_and_keys, Parameters};
    use fastcrypto::traits::KeyPair;
    use parking_lot::Mutex;
    use prometheus::Registry;
    use sui_protocol_config::ProtocolConfig;
    use tempfile::TempDir;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::sleep;

    use super::*;
    use crate::authority_node::AuthorityService;
    use crate::block::{timestamp_utc_ms, BlockRef, Round, TestBlock, VerifiedBlock};
    use crate::block_verifier::NoopBlockVerifier;
    use crate::context::Context;
    use crate::core_thread::{CoreError, CoreThreadDispatcher};
    use crate::network::NetworkClient;
    use crate::storage::mem_store::MemStore;
    use crate::transaction::NoopTransactionVerifier;

    struct FakeCoreThreadDispatcher {
        blocks: Mutex<Vec<VerifiedBlock>>,
    }

    impl FakeCoreThreadDispatcher {
        fn new() -> Self {
            Self {
                blocks: Mutex::new(vec![]),
            }
        }

        fn get_blocks(&self) -> Vec<VerifiedBlock> {
            self.blocks.lock().clone()
        }
    }

    #[async_trait]
    impl CoreThreadDispatcher for FakeCoreThreadDispatcher {
        async fn add_blocks(
            &self,
            blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            let block_refs = blocks.iter().map(|b| b.reference()).collect();
            self.blocks.lock().extend(blocks);
            Ok(block_refs)
        }

        async fn force_new_block(&self, _round: Round) -> Result<(), CoreError> {
            unimplemented!()
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeNetworkClient {}

    #[async_trait]
    impl NetworkClient for FakeNetworkClient {
        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _serialized_block: &Bytes,
        ) -> ConsensusResult<()> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_blocks(
            &self,
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }
    }

    #[tokio::test]
    async fn test_authority_start_and_stop() {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: Some(temp_dir.into_path()),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let own_index = committee.to_authority_index(0).unwrap();
        let protocol_keypair = keypairs[own_index].1.copy();
        let network_keypair = keypairs[own_index].0.copy();

        let (sender, _receiver) = unbounded_channel();
        let commit_consumer = CommitConsumer::new(
            sender, 0, // last_processed_index
        );

        let authority = ConsensusAuthority::start(
            own_index,
            committee,
            parameters,
            ProtocolConfig::get_for_max_version_UNSAFE(),
            protocol_keypair,
            network_keypair,
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
        )
        .await;

        assert_eq!(authority.0.context.own_index, own_index);
        assert_eq!(authority.0.context.committee.epoch(), 0);
        assert_eq!(authority.0.context.committee.size(), 1);

        authority.stop().await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_authority_service() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let network_client = Arc::new(FakeNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            block_verifier.clone(),
        );
        let authority_service = Arc::new(AuthorityService {
            context: context.clone(),
            block_verifier,
            core_dispatcher: core_dispatcher.clone(),
            synchronizer,
            dag_state,
        });

        // Test delaying blocks with time drift.
        let now = timestamp_utc_ms();
        let max_drift = context.parameters.max_forward_time_drift;
        let input_block = VerifiedBlock::new_for_test(
            TestBlock::new(9, 0)
                .set_timestamp_ms(now + max_drift.as_millis() as u64)
                .build(),
        );

        let service = authority_service.clone();
        let serialized = input_block.serialized().clone();
        tokio::spawn(async move {
            service
                .handle_send_block(context.committee.to_authority_index(0).unwrap(), serialized)
                .await
                .unwrap();
        });

        sleep(max_drift / 2).await;
        assert!(core_dispatcher.get_blocks().is_empty());

        sleep(max_drift).await;
        let blocks = core_dispatcher.get_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], input_block);
    }

    // TODO: build AuthorityFixture.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_authority_committee() {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1, 1, 1, 1]);
        let mut output_receivers = vec![];
        let mut authorities = vec![];
        for (index, _authority_info) in committee.authorities() {
            let registry = Registry::new();

            let temp_dir = TempDir::new().unwrap();
            let parameters = Parameters {
                db_path: Some(temp_dir.into_path()),
                ..Default::default()
            };
            let txn_verifier = NoopTransactionVerifier {};

            let protocol_keypair = keypairs[index].1.copy();
            let network_keypair = keypairs[index].0.copy();

            let (sender, receiver) = unbounded_channel();
            let commit_consumer = CommitConsumer::new(
                sender, 0, // last_processed_index
            );
            output_receivers.push(receiver);

            let authority = ConsensusAuthority::start(
                index,
                committee.clone(),
                parameters,
                ProtocolConfig::get_for_max_version_UNSAFE(),
                protocol_keypair,
                network_keypair,
                Arc::new(txn_verifier),
                commit_consumer,
                registry,
            )
            .await;
            authorities.push(authority);
        }

        const NUM_TRANSACTIONS: u8 = 15;
        let mut submitted_transactions = BTreeSet::<Vec<u8>>::new();
        for i in 0..NUM_TRANSACTIONS {
            let txn = vec![i; 16];
            submitted_transactions.insert(txn.clone());
            authorities[i as usize % authorities.len()]
                .transaction_client()
                .submit(txn)
                .await
                .unwrap();
        }

        for mut receiver in output_receivers {
            let mut expected_transactions = submitted_transactions.clone();
            loop {
                let committed_subdag =
                    tokio::time::timeout(Duration::from_secs(1), receiver.recv())
                        .await
                        .unwrap()
                        .unwrap();
                for b in committed_subdag.blocks {
                    for txn in b.transactions().iter().map(|t| t.data().to_vec()) {
                        assert!(
                            expected_transactions.remove(&txn),
                            "Transaction not submitted or already seen: {:?}",
                            txn
                        );
                    }
                }
                if expected_transactions.is_empty() {
                    break;
                }
            }
        }

        for authority in authorities {
            authority.stop().await;
        }
    }
}
