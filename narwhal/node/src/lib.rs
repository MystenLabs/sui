// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Parameters, SharedCommittee, WorkerId};
use consensus::{
    bullshark::Bullshark, dag::Dag, metrics::ConsensusMetrics, Consensus, SubscriberHandler,
};
use crypto::traits::{KeyPair, Signer, VerifyingKey};
use executor::{ExecutionState, Executor, SerializedTransaction, SubscriberResult};
use primary::{NetworkModel, PayloadToken, Primary};
use prometheus::Registry;
use std::path::Path;
use std::sync::Arc;
use store::{
    reopen,
    rocks::{open_cf, DBMap},
    Store,
};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};
use tracing::debug;
use types::{
    BatchDigest, Certificate, CertificateDigest, ConsensusPrimaryMessage, ConsensusStore, Header,
    HeaderDigest, Round, SequenceNumber, SerializedBatchMessage,
};
use worker::{metrics::initialise_metrics, Worker};

pub mod execution_state;
pub mod metrics;

/// All the data stores of the node.
pub struct NodeStorage<PublicKey: VerifyingKey> {
    pub header_store: Store<HeaderDigest, Header<PublicKey>>,
    pub certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
    pub payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    pub batch_store: Store<BatchDigest, SerializedBatchMessage>,
    pub consensus_store: Arc<ConsensusStore<PublicKey>>,
}

impl<PublicKey: VerifyingKey> NodeStorage<PublicKey> {
    /// The datastore column family names.
    const HEADERS_CF: &'static str = "headers";
    const CERTIFICATES_CF: &'static str = "certificates";
    const PAYLOAD_CF: &'static str = "payload";
    const BATCHES_CF: &'static str = "batches";
    const LAST_COMMITTED_CF: &'static str = "last_committed";
    const SEQUENCE_CF: &'static str = "sequence";

    /// Open or reopen all the storage of the node.
    pub fn reopen<Path: AsRef<std::path::Path>>(store_path: Path) -> Self {
        let rocksdb = open_cf(
            store_path,
            None,
            &[
                Self::HEADERS_CF,
                Self::CERTIFICATES_CF,
                Self::PAYLOAD_CF,
                Self::BATCHES_CF,
                Self::LAST_COMMITTED_CF,
                Self::SEQUENCE_CF,
            ],
        )
        .expect("Cannot open database");

        let (header_map, certificate_map, payload_map, batch_map, last_committed_map, sequence_map) = reopen!(&rocksdb,
            Self::HEADERS_CF;<HeaderDigest, Header<PublicKey>>,
            Self::CERTIFICATES_CF;<CertificateDigest, Certificate<PublicKey>>,
            Self::PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>,
            Self::BATCHES_CF;<BatchDigest, SerializedBatchMessage>,
            Self::LAST_COMMITTED_CF;<PublicKey, Round>,
            Self::SEQUENCE_CF;<SequenceNumber, CertificateDigest>
        );

        let header_store = Store::new(header_map);
        let certificate_store = Store::new(certificate_map);
        let payload_store = Store::new(payload_map);
        let batch_store = Store::new(batch_map);
        let consensus_store = Arc::new(ConsensusStore::new(last_committed_map, sequence_map));

        Self {
            header_store,
            certificate_store,
            payload_store,
            batch_store,
            consensus_store,
        }
    }
}

/// High level functions to spawn the primary and the workers.
pub struct Node;

impl Node {
    /// The default channel capacity.
    pub const CHANNEL_CAPACITY: usize = 1_000;

    /// Spawn a new primary. Optionally also spawn the consensus and a client executing transactions.
    pub async fn spawn_primary<Keys, PublicKey, State>(
        // The private-public key pair of this authority.
        keypair: Keys,
        // The committee information.
        committee: SharedCommittee<PublicKey>,
        // The node's storage.
        store: &NodeStorage<PublicKey>,
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
        // A channel to output transactions execution confirmations.
        tx_confirmation: Sender<(SubscriberResult<Vec<u8>>, SerializedTransaction)>,
        // A prometheus exporter Registry to use for the metrics
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        Keys: KeyPair<PubKey = PublicKey> + Signer<PublicKey::Sig> + Send + 'static,
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_new_certificates, rx_new_certificates) = channel(Self::CHANNEL_CAPACITY);
        let (tx_consensus, rx_consensus) = channel(Self::CHANNEL_CAPACITY);

        // Compute the public key of this authority.
        let name = keypair.public().clone();
        let mut handlers = Vec::new();

        let (dag, network_model) = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Tusk");
            let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
            let (_handle, dag) =
                Dag::new(&*committee.load(), rx_new_certificates, consensus_metrics);
            (Some(Arc::new(dag)), NetworkModel::Asynchronous)
        } else {
            handlers = Self::spawn_consensus(
                name.clone(),
                committee.clone(),
                store,
                parameters.clone(),
                execution_state,
                rx_new_certificates,
                tx_consensus.clone(),
                tx_confirmation,
                registry,
            )
            .await?;
            (None, NetworkModel::PartiallySynchronous)
        };

        // Inject memory profiling here if we build with dhat-heap feature flag
        // Put name of primary in heap profile to distinguish diff primaries
        #[cfg(feature = "dhat-heap")]
        let profiler = {
            use crypto::traits::EncodeDecodeBase64;
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
            committee.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.payload_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_consensus,
            /* dag */ dag,
            network_model,
            tx_consensus,
            registry,
        );

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

        handlers.extend(primary_handles);
        Ok(primary_handles)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<PublicKey, State>(
        name: PublicKey,
        committee: SharedCommittee<PublicKey>,
        store: &NodeStorage<PublicKey>,
        parameters: Parameters,
        execution_state: Arc<State>,
        rx_new_certificates: Receiver<Certificate<PublicKey>>,
        tx_feedback: Sender<ConsensusPrimaryMessage<PublicKey>>,
        tx_confirmation: Sender<(SubscriberResult<Vec<u8>>, SerializedTransaction)>,
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_sequence, rx_sequence) = channel(Self::CHANNEL_CAPACITY);
        let (tx_consensus_to_client, rx_consensus_to_client) = channel(Self::CHANNEL_CAPACITY);
        let (tx_client_to_consensus, rx_client_to_consensus) = channel(Self::CHANNEL_CAPACITY);
        let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));

        // Spawn the consensus core who only sequences transactions.
        let ordering_engine = Bullshark {
            committee: committee.clone(),
            store: store.consensus_store.clone(),
            gc_depth: parameters.gc_depth,
        };
        let consensus_handler = Consensus::spawn(
            committee.clone(),
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            /* rx_primary */ rx_new_certificates,
            /* tx_primary */ tx_feedback,
            /* tx_output */ tx_sequence,
            ordering_engine,
            consensus_metrics.clone(),
            parameters.gc_depth,
        );

        // The subscriber handler receives the ordered sequence from consensus and feed them
        // to the executor. The executor has its own state and data store who may crash
        // independently of the narwhal node.
        let subscriber_handler = SubscriberHandler::spawn(
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            rx_sequence,
            /* rx_client */ rx_client_to_consensus,
            /* tx_client */ tx_consensus_to_client,
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        let executor_handlers = Executor::spawn(
            name,
            committee,
            store.batch_store.clone(),
            execution_state,
            /* rx_consensus */ rx_consensus_to_client,
            /* tx_consensus */ tx_client_to_consensus,
            /* tx_output */ tx_confirmation,
        )
        .await?;

        let mut handlers = vec![consensus_handler, subscriber_handler];
        handlers.extend(executor_handlers);

        Ok(handlers)
    }

    /// Spawn a specified number of workers.
    pub fn spawn_workers<PublicKey: VerifyingKey>(
        // The public key of this authority.
        name: PublicKey,
        // The ids of the validators to spawn.
        ids: Vec<WorkerId>,
        // The committee information.
        committee: SharedCommittee<PublicKey>,
        // The node's storage,
        store: &NodeStorage<PublicKey>,
        // The configuration parameters.
        parameters: Parameters,
        // The prometheus metrics Registry
        registry: &Registry,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        let metrics = initialise_metrics(registry);

        for id in ids {
            let worker_handles = Worker::spawn(
                name.clone(),
                id,
                committee.clone(),
                parameters.clone(),
                store.batch_store.clone(),
                metrics.clone(),
            );
            handles.extend(worker_handles);
        }
        handles
    }
}
