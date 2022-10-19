// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use consensus::{
    bullshark::Bullshark,
    dag::Dag,
    metrics::{ChannelMetrics, ConsensusMetrics},
    Consensus, ConsensusOutput,
};

use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::{get_restored_consensus_output, ExecutionState, Executor, SubscriberResult};
use fastcrypto::traits::{KeyPair as _, VerifyingKey};
use itertools::Itertools;
use network::P2pNetwork;
use primary::{NetworkModel, PayloadToken, Primary, PrimaryChannelMetrics};
use prometheus::{IntGauge, Registry};
use std::sync::Arc;
use storage::{CertificateStore, ProposerKey, ProposerStore};
use store::{
    reopen,
    rocks::{open_cf, DBMap},
    Store,
};
use tokio::sync::oneshot;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info};
use types::{
    metered_channel, Batch, BatchDigest, Certificate, CertificateDigest, ConsensusStore, Header,
    HeaderDigest, ReconfigureNotification, Round, RoundVoteDigestPair, SequenceNumber,
};
use worker::{metrics::initialise_metrics, Worker};

pub mod execution_state;
pub mod metrics;
pub mod restarter;

/// All the data stores of the node.
pub struct NodeStorage {
    pub proposer_store: ProposerStore,
    pub vote_digest_store: Store<PublicKey, RoundVoteDigestPair>,
    pub header_store: Store<HeaderDigest, Header>,
    pub certificate_store: CertificateStore,
    pub payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    pub batch_store: Store<BatchDigest, Batch>,
    pub consensus_store: Arc<ConsensusStore>,
    pub temp_batch_store: Store<(CertificateDigest, BatchDigest), Batch>,
}

impl NodeStorage {
    /// The datastore column family names.
    const LAST_PROPOSED_CF: &'static str = "last_proposed";
    const VOTES_CF: &'static str = "votes";
    const HEADERS_CF: &'static str = "headers";
    const CERTIFICATES_CF: &'static str = "certificates";
    const CERTIFICATE_ID_BY_ROUND_CF: &'static str = "certificate_id_by_round";
    const CERTIFICATE_ID_BY_ORIGIN_CF: &'static str = "certificate_id_by_origin";
    const PAYLOAD_CF: &'static str = "payload";
    const BATCHES_CF: &'static str = "batches";
    const LAST_COMMITTED_CF: &'static str = "last_committed";
    const SEQUENCE_CF: &'static str = "sequence";
    const TEMP_BATCH_CF: &'static str = "temp_batches";

    /// Open or reopen all the storage of the node.
    pub fn reopen<Path: AsRef<std::path::Path>>(store_path: Path) -> Self {
        let rocksdb = open_cf(
            store_path,
            None,
            &[
                Self::LAST_PROPOSED_CF,
                Self::VOTES_CF,
                Self::HEADERS_CF,
                Self::CERTIFICATES_CF,
                Self::CERTIFICATE_ID_BY_ROUND_CF,
                Self::CERTIFICATE_ID_BY_ORIGIN_CF,
                Self::PAYLOAD_CF,
                Self::BATCHES_CF,
                Self::LAST_COMMITTED_CF,
                Self::SEQUENCE_CF,
                Self::TEMP_BATCH_CF,
            ],
        )
        .expect("Cannot open database");

        let (
            last_proposed_map,
            votes_map,
            header_map,
            certificate_map,
            certificate_id_by_round_map,
            certificate_id_by_origin_map,
            payload_map,
            batch_map,
            last_committed_map,
            sequence_map,
            temp_batch_map,
        ) = reopen!(&rocksdb,
            Self::LAST_PROPOSED_CF;<ProposerKey, Header>,
            Self::VOTES_CF;<PublicKey, RoundVoteDigestPair>,
            Self::HEADERS_CF;<HeaderDigest, Header>,
            Self::CERTIFICATES_CF;<CertificateDigest, Certificate>,
            Self::CERTIFICATE_ID_BY_ROUND_CF;<(Round, PublicKey), CertificateDigest>,
            Self::CERTIFICATE_ID_BY_ORIGIN_CF;<(PublicKey, Round), CertificateDigest>,
            Self::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
            Self::BATCHES_CF;<BatchDigest, Batch>,
            Self::LAST_COMMITTED_CF;<PublicKey, Round>,
            Self::SEQUENCE_CF;<SequenceNumber, CertificateDigest>,
            Self::TEMP_BATCH_CF;<(CertificateDigest, BatchDigest), Batch>
        );

        let proposer_store = ProposerStore::new(last_proposed_map);
        let vote_digest_store = Store::new(votes_map);
        let header_store = Store::new(header_map);
        let certificate_store = CertificateStore::new(
            certificate_map,
            certificate_id_by_round_map,
            certificate_id_by_origin_map,
        );
        let payload_store = Store::new(payload_map);
        let batch_store = Store::new(batch_map);
        let consensus_store = Arc::new(ConsensusStore::new(last_committed_map, sequence_map));
        let temp_batch_store = Store::new(temp_batch_map);

        Self {
            proposer_store,
            vote_digest_store,
            header_store,
            certificate_store,
            payload_store,
            batch_store,
            consensus_store,
            temp_batch_store,
        }
    }
}

/// High level functions to spawn the primary and the workers.
pub struct Node;

impl Node {
    /// The default channel capacity.
    pub const CHANNEL_CAPACITY: usize = 1_000;

    /// Spawn a new primary. Optionally also spawn the consensus and a client executing transactions.
    pub async fn spawn_primary<State>(
        // The private-public key pair of this authority.
        keypair: KeyPair,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's storage.
        store: &NodeStorage,
        // The configuration parameters.
        parameters: Parameters,
        // Whether to run consensus (and an executor client) or not.
        // If true, an internal consensus will be used, else an external consensus will be used.
        // If an external consensus will be used, then this bool will also ensure that the
        // corresponding gRPC server that is used for communication between narwhal and
        // external consensus is also spawned.
        internal_consensus: bool,
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
        // A prometheus exporter Registry to use for the metrics
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let initial_committee = ReconfigureNotification::NewEpoch((**committee.load()).clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        // These gauge is porcelain: do not modify it without also modifying `primary::metrics::PrimaryChannelMetrics::replace_registered_new_certificates_metric`
        // This hack avoids a cyclic dependency in the initialization of consensus and primary
        let new_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_NEW_CERTS,
            PrimaryChannelMetrics::DESC_NEW_CERTS,
        )
        .unwrap();
        let (tx_new_certificates, rx_new_certificates) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &new_certificates_counter);

        let committed_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
            PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
        )
        .unwrap();
        let (tx_consensus, rx_consensus) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &committed_certificates_counter);

        // Compute the public key of this authority.
        let name = keypair.public().clone();
        let mut handles = Vec::new();
        let (rx_executor_network, tx_executor_network) = oneshot::channel();
        let (dag, network_model) = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Bullshark");
            let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
            let (handle, dag) = Dag::new(&committee.load(), rx_new_certificates, consensus_metrics);

            handles.push(handle);

            (Some(Arc::new(dag)), NetworkModel::Asynchronous)
        } else {
            let consensus_handles = Self::spawn_consensus(
                name.clone(),
                tx_executor_network,
                worker_cache.clone(),
                committee.clone(),
                store,
                parameters.clone(),
                execution_state,
                &tx_reconfigure,
                rx_new_certificates,
                tx_consensus.clone(),
                registry,
            )
            .await?;

            handles.extend(consensus_handles);
            (None, NetworkModel::PartiallySynchronous)
        };

        // Inject memory profiling here if we build with dhat-heap feature flag
        // Put name of primary in heap profile to distinguish diff primaries
        #[cfg(feature = "dhat-heap")]
        let profiler = {
            use fastcrypto::traits::EncodeDecodeBase64;
            use std::path::Path;

            let heap_file = format!("dhat-heap-{}.json", name.encode_base64());
            Arc::new(
                dhat::Profiler::builder()
                    .file_name(Path::new(&heap_file))
                    .build(),
            )
        };

        // Spawn the primary.
        let primary_handles = Primary::spawn(
            name.clone(),
            keypair,
            network_keypair,
            committee.clone(),
            worker_cache.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            store.consensus_store.clone(),
            tx_new_certificates,
            rx_consensus,
            dag,
            network_model,
            tx_reconfigure,
            tx_consensus,
            registry,
            Some(rx_executor_network),
        );
        handles.extend(primary_handles);

        // Let's spin off a separate thread that waits a while then dumps the profile,
        // otherwise this function exits immediately and the profile is dumped way too soon.
        // See https://github.com/nnethercote/dhat-rs/issues/19 for a panic that happens,
        // but at least 2 primaries should complete and dump their profiles.
        #[cfg(feature = "dhat-heap")]
        {
            use std::time::Duration;

            #[allow(clippy::redundant_clone)]
            let profiler2 = profiler.clone();
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_secs(240));
                println!("Dropping DHAT profiler...");
                drop(profiler2);
            });
        }

        Ok(handles)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<State>(
        name: PublicKey,
        network: oneshot::Receiver<P2pNetwork>,
        worker_cache: SharedWorkerCache,
        committee: SharedCommittee,
        store: &NodeStorage,
        parameters: Parameters,
        execution_state: State,
        tx_reconfigure: &watch::Sender<ReconfigureNotification>,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_feedback: metered_channel::Sender<Certificate>,
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
        let channel_metrics = ChannelMetrics::new(registry);

        let (tx_sequence, rx_sequence) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &channel_metrics.tx_sequence);

        // Check for any certs that have been sent by consensus but were not processed by the executor.
        let restored_consensus_output = get_restored_consensus_output(
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            &execution_state,
        )
        .await?
        .into_iter()
        .sorted_by(|a, b| a.consensus_index.cmp(&b.consensus_index))
        .collect::<Vec<ConsensusOutput>>();

        let len_restored = restored_consensus_output.len() as u64;
        if len_restored > 0 {
            info!(
                "Consensus output on its way to the executor was restored for {} certificates",
                len_restored
            );
        }
        consensus_metrics
            .recovered_consensus_output
            .inc_by(len_restored);

        // Spawn the consensus core who only sequences transactions.
        let ordering_engine = Bullshark::new(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            parameters.gc_depth,
        );
        let consensus_handles = Consensus::spawn(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            tx_reconfigure.subscribe(),
            /* rx_primary */ rx_new_certificates,
            /* tx_primary */ tx_feedback,
            /* tx_output */ tx_sequence,
            ordering_engine,
            consensus_metrics.clone(),
            parameters.gc_depth,
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        let executor_handles = Executor::spawn(
            name,
            network,
            worker_cache,
            (**committee.load()).clone(),
            execution_state,
            tx_reconfigure,
            /* rx_consensus */ rx_sequence,
            registry,
            restored_consensus_output,
        )?;

        Ok(executor_handles
            .into_iter()
            .chain(std::iter::once(consensus_handles))
            .collect())
    }

    /// Spawn a specified number of workers.
    pub fn spawn_workers(
        // The public key of this authority.
        primary_name: PublicKey,
        // The ids & keypairs of the workers to spawn.
        ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's storage,
        store: &NodeStorage,
        // The configuration parameters.
        parameters: Parameters,
        // The prometheus metrics Registry
        registry: &Registry,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        let metrics = initialise_metrics(registry);

        for (id, keypair) in ids_and_keypairs {
            let worker_handles = Worker::spawn(
                primary_name.clone(),
                keypair,
                id,
                committee.clone(),
                worker_cache.clone(),
                parameters.clone(),
                store.batch_store.clone(),
                metrics.clone(),
            );
            handles.extend(worker_handles);
        }
        handles
    }
}
