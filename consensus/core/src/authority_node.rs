// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use itertools::Itertools;
use parking_lot::RwLock;
use prometheus::Registry;
use sui_protocol_config::{ConsensusNetwork, ProtocolConfig};
use tracing::{info, warn};

use crate::{
    authority_service::AuthorityService,
    block_manager::BlockManager,
    block_verifier::SignedBlockVerifier,
    broadcaster::Broadcaster,
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
        anemo_network::AnemoManager, tonic_network::TonicManager, NetworkClient as _,
        NetworkManager,
    },
    round_prober::{RoundProber, RoundProberHandle},
    storage::rocksdb_store::RocksDBStore,
    subscriber::Subscriber,
    synchronizer::{Synchronizer, SynchronizerHandle},
    transaction::{TransactionClient, TransactionConsumer, TransactionVerifier},
    CommitConsumer, CommitConsumerMonitor,
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
        // A counter that keeps track of how many times the authority node has been booted while the binary
        // or the component that is calling the `ConsensusAuthority` has been running. It's mostly useful to
        // make decisions on whether amnesia recovery should run or not. When `boot_counter` is 0, then `ConsensusAuthority`
        // will initiate the process of amnesia recovery if that's enabled in the parameters.
        boot_counter: u64,
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
                    boot_counter,
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
                    boot_counter,
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

    pub async fn replay_complete(&self) {
        match self {
            Self::WithAnemo(authority) => authority.replay_complete().await,
            Self::WithTonic(authority) => authority.replay_complete().await,
        }
    }

    #[cfg(test)]
    fn context(&self) -> &Arc<Context> {
        match self {
            Self::WithAnemo(authority) => &authority.context,
            Self::WithTonic(authority) => &authority.context,
        }
    }

    #[allow(unused)]
    fn sync_last_known_own_block_enabled(&self) -> bool {
        match self {
            Self::WithAnemo(authority) => authority.sync_last_known_own_block,
            Self::WithTonic(authority) => authority.sync_last_known_own_block,
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
    commit_consumer_monitor: Arc<CommitConsumerMonitor>,

    commit_syncer_handle: CommitSyncerHandle,
    round_prober_handle: Option<RoundProberHandle>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
    core_thread_handle: CoreThreadHandle,
    // Only one of broadcaster and subscriber gets created, depending on
    // if streaming is supported.
    broadcaster: Option<Broadcaster>,
    subscriber: Option<Subscriber<N::Client, AuthorityService<ChannelCoreThreadDispatcher>>>,
    network_manager: N,
    sync_last_known_own_block: bool,
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
        boot_counter: u64,
    ) -> Self {
        assert!(
            committee.is_valid_index(own_index),
            "Invalid own index {}",
            own_index
        );
        let own_hostname = &committee.authority(own_index).hostname;
        info!(
            "Starting consensus authority {} {}, {:?}, boot counter {}",
            own_index, own_hostname, protocol_config.version, boot_counter
        );
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
            own_index,
            committee,
            parameters,
            protocol_config,
            initialise_metrics(registry),
            Arc::new(Clock::new()),
        ));
        let start_time = Instant::now();

        let (tx_client, tx_receiver) = TransactionClient::new(context.clone());
        let tx_consumer = TransactionConsumer::new(tx_receiver, context.clone());

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

        let store_path = context.parameters.db_path.as_path().to_str().unwrap();
        let store = Arc::new(RocksDBStore::new(store_path));
        let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

        let highest_known_commit_at_startup = dag_state.read().last_commit_index();

        let sync_last_known_own_block = boot_counter == 0
            && dag_state.read().highest_accepted_round() == 0
            && !context
                .parameters
                .sync_last_known_own_block_timeout
                .is_zero();
        info!("Sync last known own block: {sync_last_known_own_block}");

        let block_verifier = Arc::new(SignedBlockVerifier::new(
            context.clone(),
            transaction_verifier,
        ));

        let block_manager =
            BlockManager::new(context.clone(), dag_state.clone(), block_verifier.clone());

        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let commit_consumer_monitor = commit_consumer.monitor();
        commit_consumer_monitor
            .set_highest_observed_commit_at_startup(highest_known_commit_at_startup);
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
            // When there is only one (this) authority, assume subscriber exists.
            !N::Client::SUPPORT_STREAMING || context.committee.size() == 1,
            commit_observer,
            core_signals,
            protocol_keypair,
            dag_state.clone(),
            sync_last_known_own_block,
        );

        let (core_dispatcher, core_thread_handle) =
            ChannelCoreThreadDispatcher::start(context.clone(), &dag_state, core);
        let core_dispatcher = Arc::new(core_dispatcher);
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher.clone(), &signals_receivers, context.clone());

        let commit_vote_monitor = Arc::new(CommitVoteMonitor::new(context.clone()));

        let synchronizer = Synchronizer::start(
            network_client.clone(),
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            block_verifier.clone(),
            dag_state.clone(),
            sync_last_known_own_block,
        );

        let commit_syncer_handle = CommitSyncer::new(
            context.clone(),
            core_dispatcher.clone(),
            commit_vote_monitor.clone(),
            commit_consumer_monitor.clone(),
            network_client.clone(),
            block_verifier.clone(),
            dag_state.clone(),
        )
        .start();

        let round_prober_handle = if context.protocol_config.consensus_round_prober() {
            Some(
                RoundProber::new(
                    context.clone(),
                    core_dispatcher.clone(),
                    dag_state.clone(),
                    network_client.clone(),
                )
                .start(),
            )
        } else {
            None
        };

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
            commit_consumer_monitor,
            leader_timeout_handle,
            core_thread_handle,
            broadcaster,
            subscriber,
            network_manager,
            sync_last_known_own_block,
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
        if let Some(round_prober_handle) = self.round_prober_handle.take() {
            round_prober_handle.stop().await;
        }
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

    pub(crate) async fn replay_complete(&self) {
        self.commit_consumer_monitor.replay_complete().await;
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use std::collections::BTreeMap;
    use std::{collections::BTreeSet, sync::Arc, time::Duration};

    use consensus_config::{local_committee_and_keys, Parameters};
    use mysten_metrics::monitored_mpsc::UnboundedReceiver;
    use prometheus::Registry;
    use rstest::rstest;
    use sui_protocol_config::ProtocolConfig;
    use tempfile::TempDir;
    use tokio::time::{sleep, timeout};
    use typed_store::DBMetrics;

    use super::*;
    use crate::block::GENESIS_ROUND;
    use crate::{block::BlockAPI as _, transaction::NoopTransactionVerifier, CommittedSubDag};

    #[rstest]
    #[tokio::test]
    async fn test_authority_start_and_stop(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
    ) {
        let (committee, keypairs) = local_committee_and_keys(0, vec![1]);
        let registry = Registry::new();

        let temp_dir = TempDir::new().unwrap();
        let parameters = Parameters {
            db_path: temp_dir.into_path(),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let own_index = committee.to_authority_index(0).unwrap();
        let protocol_keypair = keypairs[own_index].1.clone();
        let network_keypair = keypairs[own_index].0.clone();

        let (commit_consumer, _, _) = CommitConsumer::new(0);

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
            0,
        )
        .await;

        assert_eq!(authority.context().own_index, own_index);
        assert_eq!(authority.context().committee.epoch(), 0);
        assert_eq!(authority.context().committee.size(), 1);

        authority.stop().await;
    }

    // TODO: build AuthorityFixture.
    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_authority_committee(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
        #[values(0, 5, 10)] gc_depth: u32,
    ) {
        let db_registry = Registry::new();
        DBMetrics::init(&db_registry);

        const NUM_OF_AUTHORITIES: usize = 4;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.set_consensus_gc_depth_for_testing(gc_depth);

        if gc_depth == 0 {
            protocol_config.set_consensus_linearize_subdag_v2_for_testing(false);
        }

        let temp_dirs = (0..NUM_OF_AUTHORITIES)
            .map(|_| TempDir::new().unwrap())
            .collect::<Vec<_>>();

        let mut output_receivers = Vec::with_capacity(committee.size());
        let mut authorities = Vec::with_capacity(committee.size());
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];

        for (index, _authority_info) in committee.authorities() {
            let (authority, receiver) = make_authority(
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
        sleep(Duration::from_secs(10)).await;

        // Restart authority 1 and let it run.
        let (authority, receiver) = make_authority(
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
        output_receivers[index] = receiver;
        authorities.insert(index.value(), authority);
        sleep(Duration::from_secs(10)).await;

        // Stop all authorities and exit.
        for authority in authorities {
            authority.stop().await;
        }
    }

    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_small_committee(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
        #[values(1, 2, 3)] num_authorities: usize,
    ) {
        let db_registry = Registry::new();
        DBMetrics::init(&db_registry);

        let (committee, keypairs) = local_committee_and_keys(0, vec![1; num_authorities]);
        let protocol_config: ProtocolConfig = ProtocolConfig::get_for_max_version_UNSAFE();

        let temp_dirs = (0..num_authorities)
            .map(|_| TempDir::new().unwrap())
            .collect::<Vec<_>>();

        let mut output_receivers = Vec::with_capacity(committee.size());
        let mut authorities: Vec<ConsensusAuthority> = Vec::with_capacity(committee.size());
        let mut boot_counters = vec![0; num_authorities];

        for (index, _authority_info) in committee.authorities() {
            let (authority, receiver) = make_authority(
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

        // Stop authority 0.
        let index = committee.to_authority_index(0).unwrap();
        authorities.remove(index.value()).stop().await;
        sleep(Duration::from_secs(10)).await;

        // Restart authority 0 and let it run.
        let (authority, receiver) = make_authority(
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
        output_receivers[index] = receiver;
        authorities.insert(index.value(), authority);
        sleep(Duration::from_secs(10)).await;

        // Stop all authorities and exit.
        for authority in authorities {
            authority.stop().await;
        }
    }

    #[rstest]
    #[tokio::test(flavor = "current_thread")]
    async fn test_amnesia_recovery_success(
        #[values(ConsensusNetwork::Anemo, ConsensusNetwork::Tonic)] network_type: ConsensusNetwork,
        #[values(0, 5, 10)] gc_depth: u32,
    ) {
        telemetry_subscribers::init_for_testing();
        let db_registry = Registry::new();
        DBMetrics::init(&db_registry);

        const NUM_OF_AUTHORITIES: usize = 4;
        let (committee, keypairs) = local_committee_and_keys(0, [1; NUM_OF_AUTHORITIES].to_vec());
        let mut output_receivers = vec![];
        let mut authorities = BTreeMap::new();
        let mut temp_dirs = BTreeMap::new();
        let mut boot_counters = [0; NUM_OF_AUTHORITIES];

        let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        protocol_config.set_consensus_gc_depth_for_testing(gc_depth);

        if gc_depth == 0 {
            protocol_config.set_consensus_linearize_subdag_v2_for_testing(false);
        }

        for (index, _authority_info) in committee.authorities() {
            let dir = TempDir::new().unwrap();
            let (authority, receiver) = make_authority(
                index,
                &dir,
                committee.clone(),
                keypairs.clone(),
                network_type,
                boot_counters[index],
                protocol_config.clone(),
            )
            .await;
            assert!(authority.sync_last_known_own_block_enabled(), "Expected syncing of last known own block to be enabled as all authorities are of empty db and boot for first time.");
            boot_counters[index] += 1;
            output_receivers.push(receiver);
            authorities.insert(index, authority);
            temp_dirs.insert(index, dir);
        }

        // Now we take the receiver of authority 1 and we wait until we see at least one block committed from this authority
        // We wait until we see at least one committed block authored from this authority. That way we'll be 100% sure that
        // at least one block has been proposed and successfully received by a quorum of nodes.
        let index_1 = committee.to_authority_index(1).unwrap();
        'outer: while let Some(result) =
            timeout(Duration::from_secs(10), output_receivers[index_1].recv())
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
        let (authority, mut receiver) = make_authority(
            index_1,
            &dir,
            committee.clone(),
            keypairs.clone(),
            network_type,
            boot_counters[index_1],
            protocol_config.clone(),
        )
        .await;
        assert!(
            authority.sync_last_known_own_block_enabled(),
            "Authority should have the sync of last own block enabled"
        );
        boot_counters[index_1] += 1;
        authorities.insert(index_1, authority);
        temp_dirs.insert(index_1, dir);
        sleep(Duration::from_secs(5)).await;

        // Now spin up authority 2 using its earlier directly - so no amnesia recovery should be forced here.
        // Authority 1 should be able to recover from amnesia successfully.
        let (authority, _receiver) = make_authority(
            index_2,
            &temp_dirs[&index_2],
            committee.clone(),
            keypairs,
            network_type,
            boot_counters[index_2],
            protocol_config.clone(),
        )
        .await;
        assert!(
            !authority.sync_last_known_own_block_enabled(),
            "Authority should not have attempted to sync the last own block"
        );
        boot_counters[index_2] += 1;
        authorities.insert(index_2, authority);
        sleep(Duration::from_secs(5)).await;

        // We wait until we see at least one committed block authored from this authority
        'outer: while let Some(result) = receiver.recv().await {
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
        network_type: ConsensusNetwork,
        boot_counter: u64,
        protocol_config: ProtocolConfig,
    ) -> (ConsensusAuthority, UnboundedReceiver<CommittedSubDag>) {
        let registry = Registry::new();

        // Cache less blocks to exercise commit sync.
        let parameters = Parameters {
            db_path: db_dir.path().to_path_buf(),
            dag_state_cached_rounds: 5,
            commit_sync_parallel_fetches: 2,
            commit_sync_batch_size: 3,
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let protocol_keypair = keypairs[index].1.clone();
        let network_keypair = keypairs[index].0.clone();

        let (commit_consumer, commit_receiver, _) = CommitConsumer::new(0);

        let authority = ConsensusAuthority::start(
            network_type,
            index,
            committee,
            parameters,
            protocol_config,
            protocol_keypair,
            network_keypair,
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            boot_counter,
        )
        .await;

        (authority, commit_receiver)
    }
}
