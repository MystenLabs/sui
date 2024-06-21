// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use parking_lot::RwLock;
use prometheus::Registry;
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};
use tracing::info;

use crate::{
    authority_service::AuthorityService,
    block_manager::BlockManager,
    block_verifier::SignedBlockVerifier,
    broadcaster::Broadcaster,
    commit_observer::CommitObserver,
    commit_syncer::{CommitSyncer, CommitVoteMonitor},
    context::{Clock, Context},
    core::{Core, CoreSignals},
    core_thread::{ChannelCoreThreadDispatcher, CoreThreadHandle},
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
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

impl ConsensusAuthority {
    pub async fn start(
        network_type: ConsensusNetwork,
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
            ConsensusNetwork::Anemo => {
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
            ConsensusNetwork::Tonic => {
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
            Arc::new(Clock::new()),
        ));
        let start_time = Instant::now();

        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        let (core_signals, signals_receivers) = CoreSignals::new(context.clone());

        let mut network_manager = N::new(context.clone(), network_keypair);
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

        let leader_schedule = if context
            .protocol_config
            .mysticeti_leader_scoring_and_schedule()
        {
            Arc::new(LeaderSchedule::from_store(
                context.clone(),
                dag_state.clone(),
            ))
        } else {
            Arc::new(LeaderSchedule::new(
                context.clone(),
                LeaderSwapTable::default(),
            ))
        };

        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            store.clone(),
            leader_schedule.clone(),
        );

        let core = Core::new(
            context.clone(),
            leader_schedule,
            tx_consumer,
            block_manager,
            // For streaming RPC, Core will be notified when consumer is available.
            // For non-streaming RPC, there is no way to know so default to true.
            !N::Client::SUPPORT_STREAMING,
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
            dag_state.clone(),
        );

        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));
        let commit_syncer = CommitSyncer::new(
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            network_client.clone(),
            block_verifier.clone(),
            dag_state.clone(),
        );

        let network_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            commit_vote_monitor,
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

        network_manager.install_service(network_service).await;

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
        self.commit_syncer.stop().await;
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
    #![allow(non_snake_case)]

    use std::{collections::BTreeSet, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use bytes::Bytes;
    use consensus_config::{local_committee_and_keys, Parameters};
    use mysten_metrics::monitored_mpsc::unbounded_channel;
    use parking_lot::Mutex;
    use prometheus::Registry;
    use rstest::rstest;
    use sui_protocol_config::ProtocolConfig;
    use tempfile::TempDir;
    use tokio::{sync::broadcast, time::sleep};
    use typed_store::DBMetrics;

    use super::*;
    use crate::{
        authority_node::AuthorityService,
        block::{BlockAPI as _, BlockRef, Round, TestBlock, VerifiedBlock},
        block_verifier::NoopBlockVerifier,
        commit::CommitRange,
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

        async fn new_block(&self, _round: Round, _force: bool) -> Result<(), CoreError> {
            Ok(())
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            Ok(Default::default())
        }

        fn set_consumer_availability(&self, _available: bool) -> Result<(), CoreError> {
            Ok(())
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
            _highest_accepted_rounds: Vec<Round>,
            _timeout: Duration,
        ) -> ConsensusResult<Vec<Bytes>> {
            unimplemented!("Unimplemented")
        }

        async fn fetch_commits(
            &self,
            _peer: AuthorityIndex,
            _commit_range: CommitRange,
            _timeout: Duration,
        ) -> ConsensusResult<(Vec<Bytes>, Vec<Bytes>)> {
            unimplemented!("Unimplemented")
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_authority_start_and_stop(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
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

        let (sender, _receiver) = unbounded_channel("consensus_output");
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
            dag_state.clone(),
        );
        let authority_service = Arc::new(AuthorityService::new(
            context.clone(),
            block_verifier,
            Arc::new(CommitVoteMonitor::new(context.clone())),
            synchronizer,
            core_dispatcher.clone(),
            rx_block_broadcast,
            dag_state,
            store,
        ));

        // Test delaying blocks with time drift.
        let now = context.clock.timestamp_utc_ms();
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
    #[tokio::test(flavor = "current_thread")]
    async fn test_authority_committee(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
    ) {
        let db_registry = Registry::new();
        DBMetrics::init(&db_registry);

        let (committee, keypairs) = local_committee_and_keys(0, vec![1, 1, 1, 1]);
        let temp_dirs = (0..4).map(|_| TempDir::new().unwrap()).collect::<Vec<_>>();

        let make_authority = |index: AuthorityIndex| {
            let committee = committee.clone();
            let registry = Registry::new();

            // Cache less blocks to exercise commit sync.
            let parameters = Parameters {
                db_path: Some(temp_dirs[index.value()].path().to_path_buf()),
                dag_state_cached_rounds: 5,
                commit_sync_parallel_fetches: 3,
                commit_sync_batch_size: 3,
                ..Default::default()
            };
            let txn_verifier = NoopTransactionVerifier {};

            let protocol_keypair = keypairs[index].1.clone();
            let network_keypair = keypairs[index].0.clone();

            let (sender, receiver) = unbounded_channel("consensus_output");
            let commit_consumer = CommitConsumer::new(sender, 0, 0);

            async move {
                let authority = ConsensusAuthority::start(
                    network_type,
                    index,
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
                (authority, receiver)
            }
        };

        let mut output_receivers = Vec::with_capacity(committee.size());
        let mut authorities = Vec::with_capacity(committee.size());

        for (index, _authority_info) in committee.authorities() {
            let (authority, receiver) = make_authority(index).await;
            output_receivers.push(receiver);
            authorities.push(authority);
        }

        const NUM_TRANSACTIONS: u8 = 15;
        let mut submitted_transactions = BTreeSet::<Vec<u8>>::new();
        for i in 0..NUM_TRANSACTIONS {
            let txn = vec![i; 16];
            submitted_transactions.insert(txn.clone());
            authorities[i as usize % authorities.len()]
                .transaction_client()
                .submit(vec![txn])
                .await
                .unwrap();
        }

        for receiver in &mut output_receivers {
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
                assert_eq!(committed_subdag.reputation_scores_desc, vec![]);
                if expected_transactions.is_empty() {
                    break;
                }
            }
        }

        // Stop authority 1.
        let index = committee.to_authority_index(1).unwrap();
        authorities.remove(index.value()).stop().await;
        sleep(Duration::from_secs(15)).await;

        // Restart authority 1 and let it run.
        let (authority, receiver) = make_authority(index).await;
        output_receivers[index] = receiver;
        authorities.insert(index.value(), authority);
        sleep(Duration::from_secs(15)).await;

        // Stop all authorities and exit.
        for authority in authorities {
            authority.stop().await;
        }
    }
}
