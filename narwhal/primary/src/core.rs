use std::{sync::Arc, time::Duration};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use mysten_metrics::{
    metered_channel::{Receiver, Sender},
    monitored_scope, spawn_logged_monitored_task,
};
use sui_protocol_config::ProtocolConfig;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, warn};
use types::{CommittedSubDag, SignedHeader};

use crate::{
    dag_state::DagState, fetcher::HeaderFetcher, getter::HeaderGetter, metrics::PrimaryMetrics,
};

/// Core runs a loop that drives the process to accept incoming headers,
/// notify header producer, and commit in consensus when possible.
/// The logic to accept headers is delegated to `DagState`.
pub(crate) struct Core {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    protocol_config: ProtocolConfig,
    // The worker information cache.
    worker_cache: WorkerCache,
    // Stores headers accepted by this primary.
    dag_state: Arc<DagState>,
    // Fetches headers from peers.
    fetcher: HeaderFetcher,
    // Gets specific missing headers from peers.
    getter: HeaderGetter,
    // Notifies the header producer when one or more headers are accepted.
    tx_headers_accepted: watch::Sender<()>,
    // Sends committed subdag to be prepared for consensus output.
    tx_sequence: Sender<CommittedSubDag>,
    // Receives verified headers from RPC handler.
    rx_verified_header: Receiver<SignedHeader>,
    // Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
}

impl Core {
    pub fn new(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        protocol_config: ProtocolConfig,
        worker_cache: WorkerCache,
        dag_state: Arc<DagState>,
        fetcher: HeaderFetcher,
        getter: HeaderGetter,
        tx_headers_accepted: watch::Sender<()>,
        tx_sequence: Sender<CommittedSubDag>,
        rx_verified_header: Receiver<SignedHeader>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        Self {
            authority_id,
            committee,
            protocol_config,
            worker_cache,
            dag_state,
            fetcher,
            getter,
            tx_headers_accepted,
            tx_sequence,
            rx_verified_header,
            metrics,
        }
    }

    pub fn spawn(mut self) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                self.run().await;
            },
            "CoreLoop"
        )
    }

    async fn run(&mut self) {
        loop {
            let Some(header) = self.rx_verified_header.recv().await else {
                info!("Core loop shutting down!");
                return;
            };

            let _scope = monitored_scope("CoreLoopIteration");
            let header_key = header.key();
            debug!("Received verified header: {}", header_key);
            let mut headers = vec![header];
            while let Ok(header) = self.rx_verified_header.try_recv() {
                debug!("Received verified header without wait: {}", header.key());
                headers.push(header);
            }

            let num_accepted = match self.dag_state.try_accept(headers) {
                Ok(n) => n,
                Err(e) => {
                    warn!("Failed to accept headers: {:?}", e);
                    0
                }
            };

            let missing = self.dag_state.missing_headers(200);
            if !missing.is_empty() {
                self.getter.get_missing(missing);
            }

            // Only fetch when a node is behind significantly.
            // TODO(narwhalceti): this needs to be byzantine resistant.
            if header_key.round() > self.dag_state.highest_accepted_round() + 50 {
                self.fetcher.try_start();
            }

            if num_accepted > 0 {
                for commit in self.dag_state.try_commit() {
                    if let Err(e) = self.tx_sequence.send(commit).await {
                        warn!("Failed to send commit to consensus, shutting down. {:?}", e);
                        return;
                    };
                }

                if let Err(e) = self.tx_headers_accepted.send(()) {
                    warn!("Failed to notify header producer, shutting down: {:?}", e);
                    return;
                }
            }
        }
    }
}
