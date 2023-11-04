use std::sync::{atomic::AtomicU64, Arc};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use network::client::NetworkClient;
use storage::{HeaderStore, PayloadStore};
use sui_protocol_config::ProtocolConfig;
use tracing::debug;
use types::{
    error::{DagError, DagResult},
    Header, HeaderAPI, SignedHeader,
};

use crate::metrics::PrimaryMetrics;

struct Inner {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    protocol_config: ProtocolConfig,
    // The worker information cache.
    worker_cache: WorkerCache,
    // Highest round of certificate accepted into the certificate store.
    highest_processed_round: AtomicU64,
    // Highest round of verfied certificate that has been received.
    highest_received_round: AtomicU64,
    // Client for fetching payloads.
    client: NetworkClient,
    header_store: HeaderStore,
    // The persistent store of the available batch digests produced either via our own workers
    // or others workers.
    payload_store: PayloadStore,
    // Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
    // Send missing certificates to the `CertificateFetcher`.
    // tx_certificate_fetcher: Sender<CertificateFetcherCommand>,
    // Send certificates to be accepted into a separate task that runs
    // `process_certificates_with_lock()` in a loop.
    // See comment above `process_certificates_with_lock()` for why this is necessary.
    // tx_certificate_acceptor: Sender<(Vec<Certificate>, oneshot::Sender<DagResult<()>>, bool)>,
    // Output all certificates to the consensus layer. Must send certificates in causal order.
    // tx_new_certificates: Sender<Certificate>,
    // Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    // tx_parents: Sender<(Vec<Certificate>, Round)>,
    // A background task that synchronizes batches. A tuple of a header and the maximum accepted
    // age is sent over.
    // tx_batch_tasks: Sender<(Header, u64)>,
}

pub struct Core {
    inner: Arc<Inner>,
}

impl Core {
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        protocol_config: ProtocolConfig,
        worker_cache: WorkerCache,
        client: NetworkClient,
        header_store: HeaderStore,
        payload_store: PayloadStore,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let inner = Inner {
            authority_id,
            committee,
            protocol_config,
            worker_cache,
            highest_processed_round: AtomicU64::new(0),
            highest_received_round: AtomicU64::new(0),
            client,
            header_store,
            payload_store,
            metrics,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn try_accept(&self, _header: SignedHeader) -> DagResult<()> {
        Ok(())
    }
}
