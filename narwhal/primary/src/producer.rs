use std::{collections::VecDeque, mem::swap, sync::Arc, time::Duration};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use crypto::{traits::Signer, NetworkKeyPair};
use mysten_metrics::{metered_channel::Receiver, spawn_logged_monitored_task};
use sui_protocol_config::ProtocolConfig;
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{sleep, sleep_until, Instant},
};
use tracing::{debug, error, info};
use types::{
    now, Header, HeaderAPI, HeaderKey, HeaderSignatureBytes, HeaderV3, Round, SignedHeader,
    TimestampMs,
};

use crate::{
    broadcaster::Broadcaster, dag_state::DagState, metrics::PrimaryMetrics,
    proposer::OurDigestMessage,
};

/// Producer creates headers and broadcasts them to all peers.
pub struct Producer {
    // The id of this primary.
    authority_id: AuthorityIdentifier,
    // Key used for signing headers.
    signer: NetworkKeyPair,
    // Committee of the current epoch.
    committee: Committee,
    protocol_config: ProtocolConfig,
    // The worker information cache.
    worker_cache: WorkerCache,
    // Stores headers accepted by this primary.
    dag_state: Arc<DagState>,
    // Broadcasts headers to peers.
    broadcaster: Broadcaster,
    // Receives a notification when one or more headers are accepted.
    rx_headers_accepted: watch::Receiver<()>,
    /// Receives the batches' digests from our workers.
    rx_our_digests: Receiver<OurDigestMessage>,
    // Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
}

impl Producer {
    pub fn new(
        authority_id: AuthorityIdentifier,
        signer: NetworkKeyPair,
        committee: Committee,
        protocol_config: ProtocolConfig,
        worker_cache: WorkerCache,
        dag_state: Arc<DagState>,
        broadcaster: Broadcaster,
        rx_headers_accepted: watch::Receiver<()>,
        rx_our_digests: Receiver<OurDigestMessage>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        Self {
            authority_id,
            signer,
            committee,
            protocol_config,
            worker_cache,
            dag_state,
            broadcaster,
            rx_headers_accepted,
            rx_our_digests,
            metrics,
        }
    }

    pub fn spawn(mut self) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                self.run().await;
            },
            "ProducerLoop"
        )
    }

    // TODO(narwhalceti): remove loop and let this be driven by Core instead.
    async fn run(&mut self) {
        self.rx_headers_accepted.borrow_and_update();
        let mut own_digest_messages = VecDeque::new();
        let propose_timer = sleep_until(Instant::now() + Duration::from_millis(100));
        tokio::pin!(propose_timer);
        loop {
            tokio::select! {
                result = self.rx_our_digests.recv() => {
                    if let Some(mut message) = result {
                        if let Some(ack) = message.ack_channel.take() {
                            ack.send(()).unwrap();
                        }
                        own_digest_messages.push_back(message);
                    } else {
                        info!("Worker channel closed, shutting down!");
                        return;
                    }
                    // Do not need to trigger propose.
                    continue;
                }
                result = self.rx_headers_accepted.changed() => {
                    if result.is_err() {
                        info!("Core loop shutting down!");
                        return;
                    }
                    // Continue to trigger propose.
                }
                () = &mut propose_timer => {
                    // Continue to trigger propose.
                }
            }

            let propose_result = self.dag_state.try_propose();
            propose_timer
                .as_mut()
                .reset(Instant::now() + propose_result.next_check_delay);
            let Some((header_round, ancestors, ancestor_max_ts_ms)) =
                propose_result.header_proposal
            else {
                // DagState does not allow proposing a header. Retry later.
                continue;
            };

            // if self.authority_id.0 == 0 {
            //     println!("Proposing header at round {}", header_round);
            // }
            self.metrics.current_round.set(header_round as i64);
            let mut batch_messages =
                own_digest_messages.split_off(std::cmp::min(own_digest_messages.len(), 2000));
            swap(&mut batch_messages, &mut own_digest_messages);
            let signed_header = self
                .make_header(header_round, ancestors, ancestor_max_ts_ms, batch_messages)
                .await;

            // Accept own header int othe DAG, and persist the DAG before broadcasting.
            self.dag_state
                .try_accept(vec![signed_header.clone()])
                .unwrap();
            self.dag_state.flush();

            self.broadcaster.broadcast_header(signed_header);
        }
    }

    async fn make_header(
        &self,
        header_round: Round,
        ancestors: Vec<HeaderKey>,
        ancestor_max_ts_ms: TimestampMs,
        batch_messages: VecDeque<OurDigestMessage>,
    ) -> SignedHeader {
        self.metrics.header_parents.observe(ancestors.len() as f64);

        // Here we check that the timestamp we will include in the header is consistent with the
        // ancestors, ie our current time is *after* the timestamp in all the included headers. If
        // not we log an error and hope a kind operator fixes the clock.
        let current_time = now();
        if current_time < ancestor_max_ts_ms {
            let drift_ms = ancestor_max_ts_ms - current_time;
            error!(
                "Current time {} earlier than max ancestor time {}, sleeping for {}ms until max ancestor time.",
                current_time, ancestor_max_ts_ms, drift_ms,
            );
            self.metrics.header_max_parent_wait_ms.inc_by(drift_ms);
            sleep(Duration::from_millis(drift_ms)).await;
        }

        let header: Header = HeaderV3::new(
            self.authority_id,
            header_round,
            self.committee.epoch(),
            batch_messages
                .iter()
                .map(|m| (m.digest, (m.worker_id, m.timestamp)))
                .collect(),
            vec![],
            ancestors,
        )
        .into();

        debug!("Created header {header:?}");

        // Update metrics related to latency
        for message in batch_messages {
            let batch_inclusion_secs =
                Duration::from_millis(*header.created_at() - message.timestamp).as_secs_f64();

            // NOTE: This log entry is used to compute performance.
            tracing::debug!(
                "Batch {:?} from worker {} took {} seconds from creation to be included in a proposed header",
                message.digest,
                message.worker_id,
                batch_inclusion_secs
            );
            self.metrics
                .proposer_batch_latency
                .observe(batch_inclusion_secs);
        }

        let signature_bytes =
            HeaderSignatureBytes::from(&self.signer.sign(header.digest().as_ref()));

        SignedHeader::new(header, signature_bytes)
    }
}
