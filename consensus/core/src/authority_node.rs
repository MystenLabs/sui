// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use parking_lot::RwLock;
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tracing::info;

use crate::{
    authority_service::AuthorityService,
    block_manager::BlockManager,
    block_verifier::SignedBlockVerifier,
    broadcaster::Broadcaster,
    commit_observer::CommitObserver,
    commit_syncer::{CommitSyncer, HighestCommitMonitor},
    context::Context,
    core::{Core, CoreSignals},
    core_thread::{ChannelCoreThreadDispatcher, CoreThreadHandle},
    dag_state::DagState,
    leader_timeout::{LeaderTimeoutTask, LeaderTimeoutTaskHandle},
    metrics::initialise_metrics,
    network::{
        anemo_network::AnemoManager, tonic_network::TonicManager, NetworkClient as _,
        NetworkManager,
    },
    storage::rocksdb_store::RocksDBStore,
    subscriber::Subscriber,
    synchronizer::{Synchronizer, SynchronizerHandle},
    transaction::{TransactionClient, TransactionConsumer, TransactionVerifier},
    CommitConsumer,
};

/// ConsensusAuthority is used by Sui to manage the lifetime of AuthorityNode.
/// It hides the details of the implementation from the caller, MysticetiManager.
#[allow(private_interfaces)]
pub enum ConsensusAuthority {
    WithAnemo(AuthorityNode<AnemoManager>),
    WithTonic(AuthorityNode<TonicManager>),
}

// Type of network used by the authority node.
#[derive(Clone, Copy)]
pub enum NetworkType {
    Anemo,
    Tonic,
}

impl ConsensusAuthority {
    pub async fn start(
        network_type: NetworkType,
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
        match network_type {
            NetworkType::Anemo => {
                let authority = AuthorityNode::start(
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
                Self::WithAnemo(authority)
            }
            NetworkType::Tonic => {
                let authority = AuthorityNode::start(
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
                Self::WithTonic(authority)
            }
        }
    }

    pub async fn stop(self) {
        match self {
            Self::WithAnemo(authority) => authority.stop().await,
            Self::WithTonic(authority) => authority.stop().await,
        }
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        match self {
            Self::WithAnemo(authority) => authority.transaction_client(),
            Self::WithTonic(authority) => authority.transaction_client(),
        }
    }

    #[cfg(test)]
    fn context(&self) -> &Arc<Context> {
        match self {
            Self::WithAnemo(authority) => &authority.context,
            Self::WithTonic(authority) => &authority.context,
        }
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
    commit_syncer: CommitSyncer<N::Client>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
    core_thread_handle: CoreThreadHandle,
    // Only one of broadcaster and subscriber gets created, depending on
    // if streaming is supported.
    broadcaster: Option<Broadcaster>,
    subscriber: Option<Subscriber<N::Client, AuthorityService<ChannelCoreThreadDispatcher>>>,
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
        assert!(committee.is_valid_index(own_index));
        let context = Arc::new(Context::new(
            own_index,
            committee,
            parameters,
            protocol_config,
            initialise_metrics(registry),
        ));
        let start_time = Instant::now();

        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        let (core_signals, signals_receivers) = CoreSignals::new(context.clone());

        let mut network_manager = N::new(context.clone());
        let network_client = network_manager.client();

        // REQUIRED: Broadcaster must be created before Core, to start listening on the
        // broadcast channel in order to not miss blocks and cause test failures.
        let broadcaster = if N::Client::SUPPORT_STREAMING {
            None
        } else {
            Some(Broadcaster::new(
                context.clone(),
                network_client.clone(),
                &signals_receivers,
            ))
        };

        let store_path = context
            .parameters
            .db_path
            .as_ref()
            .expect("DB path is not set")
            .as_path()
            .to_str()
            .unwrap();
        let store = Arc::new(RocksDBStore::new(store_path));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_verifier = Arc::new(SignedBlockVerifier::new(
            context.clone(),
            transaction_verifier,
        ));

        let block_manager =
            BlockManager::new(context.clone(), dag_state.clone(), block_verifier.clone());

        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
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
        );

        let (core_dispatcher, core_thread_handle) =
            ChannelCoreThreadDispatcher::start(core, context.clone());
        let core_dispatcher = Arc::new(core_dispatcher);
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher.clone(), &signals_receivers, context.clone());

        let synchronizer = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            block_verifier.clone(),
        );

        let highest_commit_monitor = Arc::new(HighestCommitMonitor::new(&context));
        let commit_syncer = CommitSyncer::new(
            context.clone(),
            core_dispatcher.clone(),
            highest_commit_monitor.clone(),
            network_client.clone(),
            block_verifier.clone(),
            dag_state.clone(),
        );

        let network_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            highest_commit_monitor,
            synchronizer.clone(),
            core_dispatcher,
            signals_receivers.block_broadcast_receiver(),
            dag_state.clone(),
            store,
        ));

        let subscriber = if N::Client::SUPPORT_STREAMING {
            let s = Subscriber::new(
                context.clone(),
                network_client,
                network_service.clone(),
                dag_state,
            );
            for (peer, _) in context.committee.authorities() {
                if peer != context.own_index {
                    s.subscribe(peer);
                }
            }
            Some(s)
        } else {
            None
        };

        network_manager
            .install_service(network_keypair, network_service)
            .await;

        Self {
            context,
            start_time,
            transaction_client: Arc::new(tx_client),
            synchronizer,
            commit_syncer,
            leader_timeout_handle,
            core_thread_handle,
            broadcaster,
            subscriber,
            network_manager,
        }
    }

    pub(crate) async fn stop(mut self) {
        info!(
            "Stopping authority. Total run time: {:?}",
            self.start_time.elapsed()
        );

        // First shutdown components calling into Core.
        self.synchronizer.stop().await;
        self.commit_syncer.stop();
        self.leader_timeout_handle.stop().await;
        // Shutdown Core to stop block productions and broadcast.
        // When using streaming, all subscribers to broadcasted blocks stop after this.
        self.core_thread_handle.stop().await;
        if let Some(mut broadcaster) = self.broadcaster.take() {
            broadcaster.stop();
        }
        // Stop outgoing long lived streams before stopping network server.
        if let Some(subscriber) = self.subscriber.take() {
            subscriber.stop();
        }
        self.network_manager.stop().await;

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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::{local_committee_and_keys, Parameters};
    use parking_lot::Mutex;
    use prometheus::Registry;
    use rstest::rstest;
    use sui_protocol_config::ProtocolConfig;
    use tempfile::TempDir;
    use tokio::{
        sync::{broadcast, mpsc::unbounded_channel},
        time::sleep,
    };

    use super::*;
    use crate::{
        authority_node::AuthorityService,
        block::{timestamp_utc_ms, BlockAPI as _, BlockRef, Round, TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        context::Context,
        core_thread::{CoreError, CoreThreadDispatcher},
        error::ConsensusResult,
        network::{BlockStream, NetworkClient, NetworkService as _},
        storage::mem_store::MemStore,
        transaction::NoopTransactionVerifier,
    };

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
        const SUPPORT_STREAMING: bool = false;

        async fn send_block(
            &self,
            _peer: AuthorityIndex,
            _block: &VerifiedBlock,
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
            _peer: AuthorityIndex,
            _block_refs: Vec<BlockRef>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _start: Round,
            _end: Round,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_authority_start_and_stop(
        #[values(NetworkType::Anemo, NetworkType::Tonic)] network_type: NetworkType,
    ) {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: Some(temp_dir.into_path()),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let own_index = committee.to_authority_index(0).unwrap();
        let protocol_keypair = keypairs[own_index].1.clone();
        let network_keypair = keypairs[own_index].0.clone();

        let (sender, _receiver) = unbounded_channel();
        let commit_consumer = CommitConsumer::new(sender, 0, 0);

        let authority = ConsensusAuthority::start(
            network_type,
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

        assert_eq!(authority.context().own_index, own_index);
        assert_eq!(authority.context().committee.epoch(), 0);
        assert_eq!(authority.context().committee.size(), 1);

        authority.stop().await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_authority_service() {
        let (context, _keys) = Context::new_for_test(4);
        let context = Arc::new(context);
        let block_verifier = Arc::new(NoopBlockVerifier {});
        let core_dispatcher = Arc::new(FakeCoreThreadDispatcher::new());
        let (_tx_block_broadcast, rx_block_broadcast) = broadcast::channel(100);
        let network_client = Arc::new(FakeNetworkClient::default());
        let store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));
        let synchronizer = Synchronizer::start(
            network_client,
            context.clone(),
            core_dispatcher.clone(),
            block_verifier.clone(),
        );
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            Arc::new(HighestCommitMonitor::new(&context)),
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            dag_state,
            store,
        ));

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
    #[rstest]
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_authority_committee(
        #[values(NetworkType::Anemo, NetworkType::Tonic)] network_type: NetworkType,
    ) {
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

            let protocol_keypair = keypairs[index].1.clone();
            let network_keypair = keypairs[index].0.clone();

            let (sender, receiver) = unbounded_channel();
            let commit_consumer = CommitConsumer::new(sender, 0, 0);
            output_receivers.push(receiver);

            let authority = ConsensusAuthority::start(
                network_type,
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
