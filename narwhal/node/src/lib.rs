// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, Parameters, WorkerId};
use consensus::{dag::Dag, Consensus, ConsensusStore, SequenceNumber, SubscriberHandler};
use crypto::traits::{KeyPair, Signer, VerifyingKey};
use executor::{ExecutionState, Executor, SerializedTransaction, SubscriberResult};
use primary::{PayloadToken, Primary};
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
    BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, Round,
    SerializedBatchMessage,
};
use worker::Worker;

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
        committee: Committee<PublicKey>,
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
    ) -> SubscriberResult<JoinHandle<()>>
    where
        PublicKey: VerifyingKey,
        Keys: KeyPair<PubKey = PublicKey> + Signer<PublicKey::Sig> + Send + 'static,
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_new_certificates, rx_new_certificates) = channel(Self::CHANNEL_CAPACITY);
        let (tx_feedback, rx_feedback) = channel(Self::CHANNEL_CAPACITY);

        // Compute the public key of this authority.
        let name = keypair.public().clone();

        let dag = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Tusk");
            let (_handle, dag) = Dag::new(rx_new_certificates);
            Some(Arc::new(dag))
        } else {
            Self::spawn_consensus(
                name.clone(),
                committee.clone(),
                store,
                parameters.clone(),
                execution_state,
                rx_new_certificates,
                tx_feedback,
                tx_confirmation,
            )
            .await?;
            None
        };

        // Spawn the primary.
        let primary_handle = Primary::spawn(
            name.clone(),
            keypair,
            committee.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.payload_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ dag,
        );

        Ok(primary_handle)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<PublicKey, State>(
        name: PublicKey,
        committee: Committee<PublicKey>,
        store: &NodeStorage<PublicKey>,
        parameters: Parameters,
        execution_state: Arc<State>,
        rx_new_certificates: Receiver<Certificate<PublicKey>>,
        tx_feedback: Sender<Certificate<PublicKey>>,
        tx_confirmation: Sender<(SubscriberResult<Vec<u8>>, SerializedTransaction)>,
    ) -> SubscriberResult<()>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_sequence, rx_sequence) = channel(Self::CHANNEL_CAPACITY);
        let (tx_consensus_to_client, rx_consensus_to_client) = channel(Self::CHANNEL_CAPACITY);
        let (tx_client_to_consensus, rx_client_to_consensus) = channel(Self::CHANNEL_CAPACITY);

        // Spawn the consensus core who only sequences transactions.
        Consensus::spawn(
            committee.clone(),
            store.consensus_store.clone(),
            parameters.gc_depth,
            /* rx_primary */ rx_new_certificates,
            /* tx_primary */ tx_feedback,
            /* tx_output */ tx_sequence,
        );

        // The subscriber handler receives the ordered sequence from consensus and feed them
        // to the executor. The executor has its own state and data store who may crash
        // independently of the narwhal node.
        SubscriberHandler::spawn(
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            rx_sequence,
            /* rx_client */ rx_client_to_consensus,
            /* tx_client */ tx_consensus_to_client,
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        Executor::spawn(
            name,
            committee,
            store.batch_store.clone(),
            execution_state,
            /* rx_consensus */ rx_consensus_to_client,
            /* tx_consensus */ tx_client_to_consensus,
            /* tx_output */ tx_confirmation,
        )
        .await?;

        Ok(())
    }

    /// Spawn a specified number of workers.
    pub fn spawn_workers<PublicKey: VerifyingKey>(
        // The public key of this authority.
        name: PublicKey,
        // The ids of the validators to spawn.
        ids: Vec<WorkerId>,
        // The committee information.
        committee: Committee<PublicKey>,
        // The node's storage,
        store: &NodeStorage<PublicKey>,
        // The configuration parameters.
        parameters: Parameters,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for id in ids {
            handles.push(Worker::spawn(
                name.clone(),
                id,
                committee.clone(),
                parameters.clone(),
                store.batch_store.clone(),
            ));
        }
        handles
    }
}
