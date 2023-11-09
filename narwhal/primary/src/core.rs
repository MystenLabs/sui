use std::sync::Arc;

use config::{AuthorityIdentifier, Committee, WorkerCache};
use mysten_metrics::{
    metered_channel::{Receiver, Sender},
    spawn_logged_monitored_task,
};
use sui_protocol_config::ProtocolConfig;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, warn};
use types::{CommittedSubDag, SignedHeader};

use crate::{dag_state::DagState, metrics::PrimaryMetrics};

/// Core runs a loop that drives the process to accept incoming headers,
/// notify header producer, and commit in consensus when possible.
/// The logic to accept headers is delegated to `DagState`.
pub struct Core {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Committee of the current epoch.
    committee: Committee,
    protocol_config: ProtocolConfig,
    // The worker information cache.
    worker_cache: WorkerCache,
    // Stores headers accepted by this primary.
    dag_state: Arc<DagState>,
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

            debug!("Received verified header: {:?}", header);
            let mut headers = vec![header];
            while let Ok(header) = self.rx_verified_header.try_recv() {
                self.metrics
                    .highest_received_round
                    .with_label_values(&["other"])
                    .set(header.round() as i64);
                debug!("Received verified header without wait: {:?}", header);
                headers.push(header);
            }

            let num_accepted = match self.dag_state.try_accept(headers) {
                Ok(n) => n,
                Err(e) => {
                    warn!("Failed to accept headers: {:?}", e);
                    0
                }
            };

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
