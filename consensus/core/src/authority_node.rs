// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use consensus_config::{
    Committee, ConsensusProtocolConfig, NetworkKeyPair, NetworkPublicKey, Parameters,
    ProtocolKeyPair,
};
use consensus_types::block::Round;
use itertools::Itertools;
use mysten_network::Multiaddr;
use parking_lot::RwLock;
use prometheus::Registry;
use tracing::{info, warn};

use crate::{
    BlockAPI as _, CommitConsumerArgs,
    authority_service::AuthorityService,
    block_manager::BlockManager,
    block_sync_service::BlockSyncService,
    block_verifier::SignedBlockVerifier,
    commit_observer::CommitObserver,
    commit_syncer::{CommitSyncer, CommitSyncerHandle},
    commit_vote_monitor::CommitVoteMonitor,
    context::{Clock, Context},
    core::{Core, CoreSignals},
    core_thread::{ChannelCoreThreadDispatcher, CoreThreadHandle},
    dag_state::DagState,
    leader_schedule::LeaderSchedule,
    leader_timeout::{LeaderTimeoutTask, LeaderTimeoutTaskHandle},
    metrics::initialise_metrics,
    network::{
        CommitSyncerClient, NetworkManager, PeerId, SynchronizerClient, tonic_network::TonicManager,
    },
    observer_service::ObserverService,
    observer_subscriber::ObserverSubscriber,
    peers_pool::PeersPool,
    round_prober::{RoundProber, RoundProberHandle},
    round_tracker::RoundTracker,
    storage::rocksdb_store::RocksDBStore,
    subscriber::Subscriber,
    synchronizer::{Synchronizer, SynchronizerHandle},
    transaction::{TransactionClient, TransactionConsumer, TransactionVerifier},
    transaction_vote_tracker::TransactionVoteTracker,
};

/// ConsensusAuthority is used by Sui to manage the lifetime of AuthorityNode.
/// It hides the details of the implementation from the caller, MysticetiManager.
#[allow(private_interfaces)]
pub enum ConsensusAuthority {
    WithTonic(AuthorityNode<TonicManager>),
}

impl ConsensusAuthority {
    pub async fn start(
        network_type: NetworkType,
        epoch_start_timestamp_ms: u64,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ConsensusProtocolConfig,
        // Only required for validator nodes. Observer nodes don't have a protocol keypair.
        protocol_keypair: Option<ProtocolKeyPair>,
        network_keypair: NetworkKeyPair,
        clock: Arc<Clock>,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        commit_consumer: CommitConsumerArgs,
        registry: Registry,
        // A counter that keeps track of how many times the consensus authority has been booted while the process
        // has been running. It's useful for making decisions on whether amnesia recovery should run.
        // When `boot_counter` is 0, `ConsensusAuthority` will initiate the process of amnesia recovery if that's enabled in the parameters.
        boot_counter: u64,
    ) -> Self {
        match network_type {
            NetworkType::Tonic => {
                let authority = AuthorityNode::start(
                    epoch_start_timestamp_ms,
                    committee,
                    parameters,
                    protocol_config,
                    protocol_keypair,
                    network_keypair,
                    clock,
                    transaction_verifier,
                    commit_consumer,
                    registry,
                    boot_counter,
                )
                .await;
                Self::WithTonic(authority)
            }
        }
    }

    pub async fn stop(self) {
        match self {
            Self::WithTonic(authority) => authority.stop().await,
        }
    }

    pub fn update_peer_address(
        &self,
        network_pubkey: NetworkPublicKey,
        address: Option<Multiaddr>,
    ) {
        match self {
            Self::WithTonic(authority) => authority.update_peer_address(network_pubkey, address),
        }
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        match self {
            Self::WithTonic(authority) => authority.transaction_client(),
        }
    }

    #[cfg(test)]
    fn context(&self) -> &Arc<Context> {
        match self {
            Self::WithTonic(authority) => &authority.context,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetworkType {
    Tonic,
}

/// Enum to handle different subscriber types based on whether the node is a validator or observer
enum SubscriberType<N: NetworkManager> {
    Validator(Subscriber<N::ValidatorClient, AuthorityService<ChannelCoreThreadDispatcher>>),
    Observer(ObserverSubscriber<N::ObserverClient, ObserverService>),
}

impl<N: NetworkManager> SubscriberType<N> {
    fn stop(&self) {
        match self {
            SubscriberType::Validator(subscriber) => subscriber.stop(),
            SubscriberType::Observer(subscriber) => subscriber.stop(),
        }
    }
}

pub(crate) struct AuthorityNode<N>
where
    N: NetworkManager,
{
    context: Arc<Context>,
    start_time: Instant,
    transaction_client: Arc<TransactionClient>,
    synchronizer: Arc<SynchronizerHandle>,

    commit_syncer_handle: CommitSyncerHandle,
    round_prober_handle: Option<RoundProberHandle>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
    core_thread_handle: CoreThreadHandle,
    subscriber: SubscriberType<N>,
    network_manager: N,
}

impl<N> AuthorityNode<N>
where
    N: NetworkManager,
{
    // See comments above ConsensusAuthority::start() for details on the input.
    pub(crate) async fn start(
        epoch_start_timestamp_ms: u64,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ConsensusProtocolConfig,
        protocol_keypair: Option<ProtocolKeyPair>,
        network_keypair: NetworkKeyPair,
        clock: Arc<Clock>,
        transaction_verifier: Arc<dyn TransactionVerifier>,
        commit_consumer: CommitConsumerArgs,
        registry: Registry,
        boot_counter: u64,
    ) -> Self {
        let metrics = initialise_metrics(registry);

        // If a protocol key pair is provided, then this is a validator node.
        let own_index = if let Some(protocol_keypair) = &protocol_keypair {
            let (own_index, _) = committee
                .authorities()
                .find(|(_, a)| a.protocol_key == protocol_keypair.public())
                .expect("Own authority should be among the consensus authorities!");

            let own_hostname = committee.authority(own_index).hostname.clone();
            info!(
                "Starting consensus validator authority {} {}, {:?}, epoch start timestamp {}, boot counter {}, replaying after commit index {}, consumer last processed commit index {}",
                own_index,
                own_hostname,
                protocol_config.protocol_version(),
                epoch_start_timestamp_ms,
                boot_counter,
                commit_consumer.replay_after_commit_index,
                commit_consumer.consumer_last_processed_commit_index
            );

            metrics
                .node_metrics
                .authority_index
                .with_label_values(&[&own_hostname])
                .set(own_index.value() as i64);
            Some(own_index)
        } else {
            // Otherwise this is an observer node and no index exists for it.
            info!(
                "Starting consensus observer authority, {:?}, epoch start timestamp {}, boot counter {}, replaying after commit index {}, consumer last processed commit index {}",
                protocol_config.protocol_version(),
                epoch_start_timestamp_ms,
                boot_counter,
                commit_consumer.replay_after_commit_index,
                commit_consumer.consumer_last_processed_commit_index
            );
            None
        };

        info!(
            "Consensus authorities: {}",
            committee
                .authorities()
                .map(|(i, a)| format!("{}: {}", i, a.hostname))
                .join(", ")
        );
        info!("Consensus parameters: {:?}", parameters);
        info!("Consensus committee: {:?}", committee);
        let context = Arc::new(Context::new(
            epoch_start_timestamp_ms,
            own_index,
            committee,
            parameters,
            protocol_config,
            metrics,
            clock,
        ));
        let start_time = Instant::now();

        context
            .metrics
            .node_metrics
            .protocol_version
            .set(context.protocol_config.protocol_version() as i64);

        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone());

        let (core_signals, signals_receivers) = CoreSignals::new(context.clone());

        let mut network_manager = N::new(context.clone(), network_keypair);
        let validator_client = network_manager.validator_client();
        let observer_client = network_manager.observer_client();

        let synchronizer_client = Arc::new(SynchronizerClient::<
            N::ValidatorClient,
            N::ObserverClient,
        >::new(
            context.clone(),
            Some(validator_client.clone()),
            Some(observer_client.clone()),
        ));
        let commit_syncer_client = Arc::new(CommitSyncerClient::<
            N::ValidatorClient,
            N::ObserverClient,
        >::new(
            context.clone(),
            Some(validator_client.clone()),
            Some(observer_client.clone()),
        ));

        let store_path = context.parameters.db_path.as_path().to_str().unwrap();
        let store = Arc::new(RocksDBStore::new(
            store_path,
            context.parameters.use_fifo_compaction,
        ));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let block_verifier = Arc::new(SignedBlockVerifier::new(
            context.clone(),
            transaction_verifier,
        ));

        let transaction_vote_tracker =
            TransactionVoteTracker::new(context.clone(), block_verifier.clone(), dag_state.clone());

        // Only sync last known own block if we are a validator and it's the first boot.
        let sync_last_known_own_block = boot_counter == 0
            && !context
                .parameters
                .sync_last_known_own_block_timeout
                .is_zero()
            && context.is_validator();
        info!(
            "Sync last known own block: {}. Boot count: {}. Timeout: {:?}.",
            sync_last_known_own_block,
            boot_counter,
            context.parameters.sync_last_known_own_block_timeout
        );

        let block_manager = BlockManager::new(context.clone(), dag_state.clone());

        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let commit_consumer_monitor = commit_consumer.monitor();
        let commit_observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_vote_tracker.clone(),
        )
        .await;

        let initial_received_rounds = dag_state
            .read()
            .get_last_cached_block_per_authority(Round::MAX)
            .into_iter()
            .map(|(block, _)| block.round())
            .collect::<Vec<_>>();
        let round_tracker = Arc::new(RwLock::new(RoundTracker::new(
            context.clone(),
            initial_received_rounds,
        )));

        // To avoid accidentally leaking the private key, the protocol key pair should only be
        // kept in Core.
        let core = if context.is_validator() {
            Core::new_validator(
                context.clone(),
                leader_schedule,
                tx_consumer,
                transaction_vote_tracker.clone(),
                block_manager,
                commit_observer,
                core_signals,
                protocol_keypair.expect("protocol keypair is required when running as validator"),
                dag_state.clone(),
                sync_last_known_own_block,
                round_tracker.clone(),
            )
        } else {
            Core::new_observer(
                context.clone(),
                leader_schedule,
                block_manager,
                commit_observer,
                core_signals,
                dag_state.clone(),
            )
        };

        let (core_dispatcher, core_thread_handle) =
            ChannelCoreThreadDispatcher::start(context.clone(), &dag_state, core);
        let core_dispatcher = Arc::new(core_dispatcher);
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher.clone(), &signals_receivers, context.clone());

        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        // Create the PeersPool
        let peers_pool = Arc::new(PeersPool::new(context.clone()));

        let synchronizer = Synchronizer::start(
            synchronizer_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            dag_state.clone(),
            peers_pool.clone(),
            sync_last_known_own_block,
        );

        let commit_syncer_handle = CommitSyncer::new(
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            block_verifier.clone(),
            transaction_vote_tracker.clone(),
            round_tracker.clone(),
            commit_syncer_client.clone(),
            dag_state.clone(),
            peers_pool.clone(),
        )
        .start();

        // Create BlockSyncService that will be shared by both AuthorityService and ObserverService
        let block_sync_service = Arc::new(BlockSyncService::new(
            context.clone(),
            dag_state.clone(),
            store.clone(),
        ));

        let (subscriber, round_prober_handle) = if context.is_validator() {
            let authority_service = Arc::new(AuthorityService::new(
                context.clone(),
                block_verifier.clone(),
                commit_vote_monitor.clone(),
                round_tracker.clone(),
                synchronizer.clone(),
                core_dispatcher.clone(),
                signals_receivers.block_broadcast_receiver(),
                transaction_vote_tracker.clone(),
                dag_state.clone(),
                block_sync_service.clone(),
            ));

            // Start the validator server if this is a validator node.
            network_manager
                .start_validator_server(authority_service.clone())
                .await;

            // Validator node: subscribe to all other validators
            let s = Subscriber::new(
                context.clone(),
                validator_client.clone(),
                authority_service.clone(),
                dag_state.clone(),
            );
            for (peer, _) in context.committee.authorities() {
                if peer != context.own_index {
                    s.subscribe(peer);
                }
            }

            // Start the round prober
            let round_prober_handle = Some(
                RoundProber::new(
                    context.clone(),
                    core_dispatcher.clone(),
                    round_tracker.clone(),
                    dag_state.clone(),
                    validator_client,
                )
                .start(),
            );

            // Start the observer server if the observer server is enabled in the parameters.
            if context.parameters.observer.is_server_enabled() {
                let observer_service = Arc::new(ObserverService::new(
                    context.clone(),
                    core_dispatcher.clone(),
                    dag_state.clone(),
                    signals_receivers.accepted_block_broadcast_receiver(),
                    block_verifier,
                    commit_vote_monitor.clone(),
                    transaction_vote_tracker.clone(),
                    synchronizer.clone(),
                    block_sync_service.clone(),
                ));
                network_manager
                    .start_observer_server(observer_service)
                    .await;
            }

            (SubscriberType::Validator(s), round_prober_handle)
        } else {
            // Observer node: subscribe to specified peer(s) using ObserverSubscriber
            let observer_client = network_manager.observer_client();
            let observer_service = Arc::new(ObserverService::new(
                context.clone(),
                core_dispatcher.clone(),
                dag_state.clone(),
                signals_receivers.accepted_block_broadcast_receiver(),
                block_verifier,
                commit_vote_monitor.clone(),
                transaction_vote_tracker.clone(),
                synchronizer.clone(),
                block_sync_service.clone(),
            ));

            let observer_subscriber = ObserverSubscriber::new(
                context.clone(),
                observer_client,
                observer_service.clone(),
                dag_state.clone(),
            );

            network_manager
                .start_observer_server(observer_service)
                .await;

            // Subscribe to peers specified in the configuration
            // For now get the first peer from the list to connect to.
            // TODO: support multiple peers - as in choose/detect which one to connect to.
            for peer_record in context.parameters.observer.peers.iter().take(1) {
                let peer_id = if let Some((index, _)) = context
                    .committee
                    .authorities()
                    .find(|(_, authority)| authority.network_key == peer_record.public_key)
                {
                    PeerId::Validator(index)
                } else {
                    PeerId::Observer(Box::new(peer_record.public_key.clone()))
                };

                info!("Observer subscribing to peer: {:?}", peer_id);
                observer_subscriber.subscribe(peer_id);
            }

            (SubscriberType::Observer(observer_subscriber), None)
        };

        info!(
            "Consensus authority started, took {:?}",
            start_time.elapsed()
        );

        Self {
            context,
            start_time,
            transaction_client: Arc::new(tx_client),
            synchronizer,
            commit_syncer_handle,
            round_prober_handle,
            leader_timeout_handle,
            core_thread_handle,
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
        if let Err(e) = self.synchronizer.stop().await {
            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            }
            warn!(
                "Failed to stop synchronizer when shutting down consensus: {:?}",
                e
            );
        };
        self.commit_syncer_handle.stop().await;
        if let Some(round_prober_handle) = self.round_prober_handle {
            round_prober_handle.stop().await;
        }
        self.leader_timeout_handle.stop().await;
        // Shutdown Core to stop block productions and broadcast.
        self.core_thread_handle.stop().await;
        // Stop block subscriptions before stopping network server.
        self.subscriber.stop();
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

    pub(crate) fn update_peer_address(
        &self,
        network_pubkey: NetworkPublicKey,
        address: Option<Multiaddr>,
    ) {
        // Find the peer index for this network key
        let Some(peer) = self
            .context
            .committee
            .authorities()
            .find(|(_, authority)| authority.network_key == network_pubkey)
            .map(|(index, _)| index)
        else {
            warn!(
                "Network public key {:?} not found in committee, ignoring address update",
                network_pubkey
            );
            return;
        };

        // Update the address in the network manager
        self.network_manager.update_peer_address(peer, address);

        // Re-subscribe to the peer to force reconnection with new address
        if peer != self.context.own_index {
            info!("Re-subscribing to peer {} after address update", peer);
            match &self.subscriber {
                SubscriberType::Validator(s) => s.subscribe(peer),
                SubscriberType::Observer(s) => {
                    // For observer, create a PeerId for the validator
                    s.subscribe(PeerId::Validator(peer));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Arc,
        time::Duration,
    };

    use consensus_config::{
        AuthorityIndex, ObserverParameters, Parameters, PeerRecord, local_committee_and_keys,
    };
    use mysten_metrics::RegistryService;
    use mysten_metrics::monitored_mpsc::UnboundedReceiver;
    use prometheus::Registry;
    use rand::{SeedableRng, rngs::StdRng};
    use rstest::rstest;
    use tempfile::TempDir;
    use tokio::time::{sleep, timeout};
    use typed_store::DBMetrics;

    use super::*;
    use crate::{
        CommittedSubDag,
        block::{BlockAPI as _, GENESIS_ROUND},
        transaction::NoopTransactionVerifier,
    };

    #[rstest]
    #[tokio::test]
    async fn test_authority_start_and_stop(
        #[values(NetworkType::Tonic)] network_type: NetworkType,
    ) {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: temp_dir.keep(),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let own_index = committee.to_authority_index(0).unwrap();
        let protocol_keypair = keypairs[own_index].1.clone();
        let network_keypair = keypairs[own_index].0.clone();

        let (commit_consumer, _) = CommitConsumerArgs::new(0, 0);

        let authority = ConsensusAuthority::start(
            network_type,
            0,
            committee,
            parameters,
            ConsensusProtocolConfig::for_testing(),
            Some(protocol_keypair),
            network_keypair,
            Arc::new(Clock::default()),
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            0,
        )
        .await;

        assert_eq!(authority.context().own_index, own_index);
        assert_eq!(authority.context().committee.epoch(), 0);
        assert_eq!(authority.context().committee.size(), 1);

        authority.stop().await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_observer_start_and_stop(#[values(NetworkType::Tonic)] network_type: NetworkType) {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: temp_dir.keep(),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        // Use any network keypair for the observer, it doesn't need to match a committee member
        let network_keypair = keypairs[0].0.clone();

        let (commit_consumer, _) = CommitConsumerArgs::new(0, 0);

        let observer = ConsensusAuthority::start(
            network_type,
            0,
            committee.clone(),
            parameters,
            ConsensusProtocolConfig::for_testing(),
            None, // No protocol keypair for observer node
            network_keypair,
            Arc::new(Clock::default()),
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            0,
        )
        .await;

        sleep(Duration::from_secs(2)).await;

        // Observer nodes have own_index set to MAX as a special value
        assert_eq!(observer.context().own_index, AuthorityIndex::MAX);
        assert_eq!(observer.context().committee.epoch(), 0);
        assert_eq!(observer.context().committee.size(), 1);
        assert!(!observer.context().is_validator());

        observer.stop().await;
    }

    // TODO: build AuthorityFixture.
    // Spins up a committee of authorities and an observer node that connects to authority 0.
    // Verifies that the network is progressing, advancing rounds and commits. It also verifies
    // that the Observer node is receiving blocks from the network.
    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_authority_committee(
        #[values(NetworkType::Tonic)] network_type: NetworkType,
        #[values(5, 10)] gc_depth: u32,
    ) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        const NUM_OF_AUTHORITIES: usize = 4;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(gc_depth);

        let temp_dirs = (0..NUM_OF_AUTHORITIES)
            .map(|_| TempDir::new().unwrap())
            .collect::<Vec<_>>();

        let mut commit_receivers = Vec::with_capacity(committee.size());
        let mut authorities = Vec::with_capacity(committee.size());
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];

        // Use a unique port based on gc_depth to avoid conflicts between parallel tests
        let observer_server_port = 8900 + gc_depth as u16;

        // Create authorities with observer server enabled for authority 0
        let mut authority_0_network_key = None;
        for (index, authority_info) in committee.authorities() {
            let (authority, commit_receiver) = if index.value() == 0 {
                // Save authority 0's network key for Observer connection
                authority_0_network_key = Some(authority_info.network_key.clone());
                // Enable observer server for authority 0
                make_authority_with_observer_server(
                    index,
                    &temp_dirs[index.value()],
                    committee.clone(),
                    keypairs.clone(),
                    network_type,
                    boot_counters[index],
                    protocol_config.clone(),
                    Some(observer_server_port),
                )
                .await
            } else {
                make_authority(
                    index,
                    &temp_dirs[index.value()],
                    committee.clone(),
                    keypairs.clone(),
                    network_type,
                    boot_counters[index],
                    protocol_config.clone(),
                )
                .await
            };
            boot_counters[index] += 1;
            commit_receivers.push(commit_receiver);
            authorities.push(authority);
        }

        // Create an Observer node that connects to authority 0
        let observer_temp_dir = TempDir::new().unwrap();
        let mut rng = StdRng::from_seed([99; 32]);
        let observer_network_keypair = consensus_config::NetworkKeyPair::generate(&mut rng);

        let observer_parameters = Parameters {
            db_path: observer_temp_dir.path().to_path_buf(),
            observer: ObserverParameters {
                // Configure Observer to connect to authority 0
                peers: vec![PeerRecord {
                    public_key: authority_0_network_key
                        .clone()
                        .expect("Authority 0 network key should be set"),
                    address: format!("/ip4/127.0.0.1/udp/{}", observer_server_port)
                        .parse()
                        .unwrap(),
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let (observer_commit_consumer, observer_commit_receiver) = CommitConsumerArgs::new(0, 0);
        let observer = ConsensusAuthority::start(
            network_type,
            0,
            committee.clone(),
            observer_parameters,
            protocol_config.clone(),
            None, // No protocol keypair for observer
            observer_network_keypair,
            Arc::new(Clock::default()),
            Arc::new(NoopTransactionVerifier {}),
            observer_commit_consumer,
            Registry::new(),
            0,
        )
        .await;
        // The relevant endpoints are now implemented for the synchronizer and commit_syncer components, so the Observer node should be able to catch up and
        // fetch blocks beyond the latest ones that are fetched from the stream.
        commit_receivers.push(observer_commit_receiver);

        // Give Observer more time to connect and sync
        sleep(Duration::from_secs(5)).await;

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

        for receiver in &mut commit_receivers {
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

        // Stop authority 1.
        let index = committee.to_authority_index(1).unwrap();
        authorities.remove(index.value()).stop().await;
        sleep(Duration::from_secs(10)).await;

        // Restart authority 1 and let it run.
        let (authority, commit_receiver) = make_authority(
            index,
            &temp_dirs[index.value()],
            committee.clone(),
            keypairs.clone(),
            network_type,
            boot_counters[index],
            protocol_config.clone(),
        )
        .await;
        boot_counters[index] += 1;
        commit_receivers[index] = commit_receiver;
        authorities.insert(index.value(), authority);
        sleep(Duration::from_secs(10)).await;

        // Verify that the Observer node is running
        // TODO: The actual block processing for observers is not fully implemented yet
        // for now we just verify that blocks are received and the number of received blocks is not far from
        // the number of blocks sent by authority 0.
        let observer_context = observer.context();
        assert!(
            observer_context.is_observer(),
            "It should be an observer node"
        );

        // Get the total verified_blocks from authority 0 (sum across all sending authorities)
        let authority_0 = &authorities[0];
        let authority_0_context = authority_0.context();
        let mut authority_0_total_verified_blocks = 0;

        // Sum verified_blocks from all authorities as seen by authority 0
        for (_, authority_info) in committee.authorities() {
            if let Ok(metric) = authority_0_context
                .metrics
                .node_metrics
                .verified_blocks
                .get_metric_with_label_values(&[&authority_info.hostname])
            {
                authority_0_total_verified_blocks += metric.get();
                println!(
                    "authority_info.hostname: {}, metric: {:?}",
                    authority_info.hostname, authority_0_total_verified_blocks
                );
            }
        }

        let mut authority_0_total_proposed_blocks = 0;
        for force in [true, false] {
            if let Ok(metric) = authority_0_context
                .metrics
                .node_metrics
                .proposed_blocks
                .get_metric_with_label_values(&[&force.to_string()])
            {
                authority_0_total_proposed_blocks += metric.get();
            }
        }

        authority_0_total_verified_blocks += authority_0_total_proposed_blocks;

        // Sum verified_blocks from all authorities as seen by the observer
        let mut observer_received_blocks = 0;
        for (_, authority_info) in committee.authorities() {
            if let Ok(metric) = observer_context
                .metrics
                .node_metrics
                .verified_blocks
                .get_metric_with_label_values(&[&authority_info.hostname])
            {
                observer_received_blocks += metric.get();
            }
        }

        // Compare the values - they should be related but might not be exactly equal
        // due to timing and the observer connecting mid-stream
        assert!(
            observer_received_blocks > 0,
            "Observer should have received at least some blocks, got: {}",
            observer_received_blocks
        );

        println!(
            "authority_0_total_verified_blocks: {}, observer_received_blocks: {}",
            authority_0_total_verified_blocks, observer_received_blocks
        );

        const TOLERANCE: u64 = 20;
        assert!(
            authority_0_total_verified_blocks - observer_received_blocks <= TOLERANCE,
            "The number of blocks received by the observer ({}) should be close to the number of blocks verified by authority 0 ({})",
            observer_received_blocks,
            authority_0_total_verified_blocks,
        );

        // Stop observer first
        observer.stop().await;

        // Stop all authorities and exit.
        for authority in authorities {
            authority.stop().await;
        }
    }

    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_small_committee(
        #[values(NetworkType::Tonic)] network_type: NetworkType,
        #[values(1, 2, 3)] num_authorities: usize,
    ) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        let (committee, keypairs) = local_committee_and_keys(0, vec![1; num_authorities]);
        let protocol_config = ConsensusProtocolConfig::for_testing();

        let temp_dirs = (0..num_authorities)
            .map(|_| TempDir::new().unwrap())
            .collect::<Vec<_>>();

        let mut output_receivers = Vec::with_capacity(committee.size());
        let mut authorities: Vec<ConsensusAuthority> = Vec::with_capacity(committee.size());
        let mut boot_counters = vec![0; num_authorities];

        for (index, _authority_info) in committee.authorities() {
            let (authority, commit_receiver) = make_authority(
                index,
                &temp_dirs[index.value()],
                committee.clone(),
                keypairs.clone(),
                network_type,
                boot_counters[index],
                protocol_config.clone(),
            )
            .await;
            boot_counters[index] += 1;
            output_receivers.push(commit_receiver);
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
                if expected_transactions.is_empty() {
                    break;
                }
            }
        }

        // Stop authority 0.
        let index = committee.to_authority_index(0).unwrap();
        authorities.remove(index.value()).stop().await;
        sleep(Duration::from_secs(10)).await;

        // Restart authority 0 and let it run.
        let (authority, commit_receiver) = make_authority(
            index,
            &temp_dirs[index.value()],
            committee.clone(),
            keypairs.clone(),
            network_type,
            boot_counters[index],
            protocol_config.clone(),
        )
        .await;
        boot_counters[index] += 1;
        output_receivers[index] = commit_receiver;
        authorities.insert(index.value(), authority);
        sleep(Duration::from_secs(10)).await;

        // Stop all authorities and exit.
        for authority in authorities {
            authority.stop().await;
        }
    }

    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_amnesia_recovery_success(#[values(5, 10)] gc_depth: u32) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(RegistryService::new(db_registry));

        const NUM_OF_AUTHORITIES: usize = 4;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut commit_receivers = vec![];
        let mut authorities = BTreeMap::new();
        let mut temp_dirs = BTreeMap::new();
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];

        let mut protocol_config = ConsensusProtocolConfig::for_testing();
        protocol_config.set_gc_depth_for_testing(gc_depth);

        for (index, _authority_info) in committee.authorities() {
            let dir = TempDir::new().unwrap();
            let (authority, commit_receiver) = make_authority(
                index,
                &dir,
                committee.clone(),
                keypairs.clone(),
                NetworkType::Tonic,
                boot_counters[index],
                protocol_config.clone(),
            )
            .await;
            boot_counters[index] += 1;
            commit_receivers.push(commit_receiver);
            authorities.insert(index, authority);
            temp_dirs.insert(index, dir);
        }

        // Now we take the receiver of authority 1 and we wait until we see at least one block committed from this authority
        // We wait until we see at least one committed block authored from this authority. That way we'll be 100% sure that
        // at least one block has been proposed and successfully received by a quorum of nodes.
        let index_1 = committee.to_authority_index(1).unwrap();
        'outer: while let Some(result) =
            timeout(Duration::from_secs(10), commit_receivers[index_1].recv())
                .await
                .expect("Timed out while waiting for at least one committed block from authority 1")
        {
            for block in result.blocks {
                if block.round() > GENESIS_ROUND && block.author() == index_1 {
                    break 'outer;
                }
            }
        }

        // Stop authority 1 & 2.
        // * Authority 1 will be used to wipe out their DB and practically "force" the amnesia recovery.
        // * Authority 2 is stopped in order to simulate less than f+1 availability which will
        // make authority 1 retry during amnesia recovery until it has finally managed to successfully get back f+1 responses.
        // once authority 2 is up and running again.
        authorities.remove(&index_1).unwrap().stop().await;
        let index_2 = committee.to_authority_index(2).unwrap();
        authorities.remove(&index_2).unwrap().stop().await;
        sleep(Duration::from_secs(5)).await;

        // Authority 1: create a new directory to simulate amnesia. The node will start having participated previously
        // to consensus but now will attempt to synchronize the last own block and recover from there. It won't be able
        // to do that successfully as authority 2 is still down.
        let dir = TempDir::new().unwrap();
        // We do reset the boot counter for this one to simulate a "binary" restart
        boot_counters[index_1] = 0;
        let (authority, mut commit_receiver) = make_authority(
            index_1,
            &dir,
            committee.clone(),
            keypairs.clone(),
            NetworkType::Tonic,
            boot_counters[index_1],
            protocol_config.clone(),
        )
        .await;
        boot_counters[index_1] += 1;
        authorities.insert(index_1, authority);
        temp_dirs.insert(index_1, dir);
        sleep(Duration::from_secs(5)).await;

        // Now spin up authority 2 using its earlier directly - so no amnesia recovery should be forced here.
        // Authority 1 should be able to recover from amnesia successfully.
        let (authority, _commit_receiver) = make_authority(
            index_2,
            &temp_dirs[&index_2],
            committee.clone(),
            keypairs,
            NetworkType::Tonic,
            boot_counters[index_2],
            protocol_config.clone(),
        )
        .await;
        boot_counters[index_2] += 1;
        authorities.insert(index_2, authority);
        sleep(Duration::from_secs(5)).await;

        // We wait until we see at least one committed block authored from this authority
        'outer: while let Some(result) = commit_receiver.recv().await {
            for block in result.blocks {
                if block.round() > GENESIS_ROUND && block.author() == index_1 {
                    break 'outer;
                }
            }
        }

        // Stop all authorities and exit.
        for (_, authority) in authorities {
            authority.stop().await;
        }
    }

    // TODO: create a fixture
    async fn make_authority(
        index: AuthorityIndex,
        db_dir: &TempDir,
        committee: Committee,
        keypairs: Vec<(NetworkKeyPair, ProtocolKeyPair)>,
        network_type: NetworkType,
        boot_counter: u64,
        protocol_config: ConsensusProtocolConfig,
    ) -> (ConsensusAuthority, UnboundedReceiver<CommittedSubDag>) {
        make_authority_with_observer_server(
            index,
            db_dir,
            committee,
            keypairs,
            network_type,
            boot_counter,
            protocol_config,
            None, // No observer server port
        )
        .await
    }

    async fn make_authority_with_observer_server(
        index: AuthorityIndex,
        db_dir: &TempDir,
        committee: Committee,
        keypairs: Vec<(NetworkKeyPair, ProtocolKeyPair)>,
        network_type: NetworkType,
        boot_counter: u64,
        protocol_config: ConsensusProtocolConfig,
        observer_server_port: Option<u16>,
    ) -> (ConsensusAuthority, UnboundedReceiver<CommittedSubDag>) {
        let registry = Registry::new();

        // Cache less blocks to exercise commit sync.
        let mut parameters = Parameters {
            db_path: db_dir.path().to_path_buf(),
            dag_state_cached_rounds: 5,
            commit_sync_parallel_fetches: 2,
            commit_sync_batch_size: 3,
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        };

        // Enable observer server if port is provided
        if let Some(port) = observer_server_port {
            parameters.observer.server_port = Some(port);
        }

        let txn_verifier = NoopTransactionVerifier {};

        let protocol_keypair = keypairs[index].1.clone();
        let network_keypair = keypairs[index].0.clone();

        let (commit_consumer, commit_receiver) = CommitConsumerArgs::new(0, 0);

        let authority = ConsensusAuthority::start(
            network_type,
            0,
            committee,
            parameters,
            protocol_config,
            Some(protocol_keypair),
            network_keypair,
            Arc::new(Clock::default()),
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            boot_counter,
        )
        .await;

        (authority, commit_receiver)
    }
}
