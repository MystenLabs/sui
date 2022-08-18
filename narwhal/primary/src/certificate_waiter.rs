// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::PrimaryMetrics;
use config::Committee;
use dashmap::DashMap;
use futures::future::try_join_all;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::info;
use types::{
    bounded_future_queue::BoundedFuturesUnordered,
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    try_fut_and_permit, Certificate, CertificateDigest, HeaderDigest, ReconfigureNotification,
    Round,
};

#[cfg(test)]
#[path = "tests/certificate_waiter_tests.rs"]
pub mod certificate_waiter_tests;

/// The resolution of the timer that checks whether we received replies to our parent requests, and triggers
/// a round of GC if we didn't.
const GC_RESOLUTION: u64 = 10_000;

/// Waits to receive all the ancestors of a certificate before looping it back to the `Core`
/// for further processing.
pub struct CertificateWaiter {
    /// The committee information.
    committee: Committee,
    /// The persistent storage.
    store: Store<CertificateDigest, Certificate>,
    /// Receiver for signal of round change
    rx_consensus_round_updates: watch::Receiver<u64>,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Watch channel notifying of epoch changes, it is only used for cleanup.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receives sync commands from the `Synchronizer`.
    rx_synchronizer: Receiver<Certificate>,
    /// Loops back to the core certificates for which we got all parents.
    tx_core: Sender<Certificate>,
    /// List of digests (certificates) that are waiting to be processed. Their processing will
    /// resume when we get all their dependencies. The map holds a cancellation `Sender`
    /// which we can use to give up on a certificate.
    // TODO: remove the OnceCell once drain_filter stabilizes
    pending: DashMap<HeaderDigest, (Round, OnceCell<oneshot::Sender<()>>)>,
    /// The metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl CertificateWaiter {
    /// Returns the max amount of pending certificates we should expect. In the worst case of causal completion,
    /// this can be `self.gc_depth` x `self.committee.len()`
    pub fn max_pending_certificates(&self) -> usize {
        self.gc_depth as usize * self.committee.size() * 2
    }

    #[must_use]
    pub fn spawn(
        committee: Committee,
        store: Store<CertificateDigest, Certificate>,
        rx_consensus_round_updates: watch::Receiver<u64>,
        gc_depth: Round,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_synchronizer: Receiver<Certificate>,
        tx_core: Sender<Certificate>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                committee,
                store,
                rx_consensus_round_updates,
                gc_depth,
                rx_reconfigure,
                rx_synchronizer,
                tx_core,
                pending: DashMap::new(),
                metrics,
            }
            .run()
            .await;
        })
    }

    /// Helper function. It waits for particular data to become available in the storage and then
    /// delivers the specified header.
    async fn waiter(
        missing: Vec<CertificateDigest>,
        store: &Store<CertificateDigest, Certificate>,
        deliver: Certificate,
        cancel_handle: oneshot::Receiver<()>,
    ) -> DagResult<Certificate> {
        let waiting: Vec<_> = missing.into_iter().map(|x| store.notify_read(x)).collect();

        tokio::select! {
            result = try_join_all(waiting) => {
                result.map(|_| deliver).map_err(DagError::from)
            }
            // the request for this certificate is obsolete, for instance because its round is obsolete (GC'd).
            _ = cancel_handle => Ok(deliver),
        }
    }

    async fn run(&mut self) {
        let mut waiting = BoundedFuturesUnordered::with_capacity(self.max_pending_certificates());
        let timer = sleep(Duration::from_millis(GC_RESOLUTION));
        tokio::pin!(timer);
        let mut attempt_garbage_collection;

        info!("CertificateWaiter has started successfully.");
        loop {
            // Initially set to not garbage collect
            attempt_garbage_collection = false;

            tokio::select! {
                // We only accept new elements if we have "room" for them
                Some(certificate) = self.rx_synchronizer.recv(), if waiting.available_permits() > 0 => {
                    if certificate.epoch() < self.committee.epoch() {
                        continue;
                    }

                    // Ensure we process only once this certificate.
                    let header_id = certificate.header.id;
                    if self.pending.contains_key(&header_id) {
                        continue;
                    }

                    // Add the certificate to the waiter pool. The waiter will return it to us when
                    // all its parents are in the store.
                    let wait_for = certificate.header.parents.iter().cloned().collect();
                    let (tx_cancel, rx_cancel) = oneshot::channel();
                    // TODO: remove all this once drain_filter is stabilized.
                    let once_cancel = {
                        let inner = OnceCell::new();
                        inner.set(tx_cancel).expect("OnceCell invariant violated");
                        inner
                    };
                    self.pending.insert(header_id, (certificate.round(), once_cancel));
                    let fut = Self::waiter(wait_for, &self.store, certificate, rx_cancel);
                    waiting.push(fut).await;
                }
                // we poll the availability of a slot to send the result to the core simultaneously
                (Some(certificate), permit) = try_fut_and_permit!(waiting.try_next(), self.tx_core) => {
                        // TODO [issue #115]: To ensure crash-recovery of consensus, it is not enough to send every
                        // certificate for which their ancestors are in the storage. After recovery, we may also
                        // need to send a all parents certificates with rounds greater then `last_committed`.
                        permit.send(certificate);
                },
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow_and_update().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(committee) => {
                            self.committee = committee;
                            self.pending.clear();
                        },
                        ReconfigureNotification::UpdateCommittee(committee) => {
                            self.committee = committee;
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    tracing::debug!("Committee updated to {}", self.committee);

                }
                () = &mut timer => {
                    // We still would like to GC even if we do not receive anything from either the synchronizer or
                    //  the Waiter. This can happen, as we may have asked for certificates that (at the time of ask)
                    // were not past our GC bound, but which round swept up past our GC bound as we advanced rounds
                    // & caught up with the network.
                    // Those certificates may get stuck in pending, unless we periodically clean up.

                    // Reschedule the timer.
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(GC_RESOLUTION));
                    attempt_garbage_collection = true;
                },

                Ok(()) = self.rx_consensus_round_updates.changed() => {
                    attempt_garbage_collection = true;
                }

            }

            // Either upon time-out or round change
            if attempt_garbage_collection {
                let round = *self.rx_consensus_round_updates.borrow();
                if round > self.gc_depth {
                    let gc_round = round - self.gc_depth;

                    self.pending.retain(|_digest, (r, once_cancel)| {
                        if *r <= gc_round {
                            // note: this send can fail, harmlessly, if the certificate has been delivered (`notify_read`)
                            // and the present code path fires before the corresponding `waiting` item is unpacked above.
                            let _ = once_cancel
                                .take()
                                .expect("This should be protected by a write lock")
                                .send(());
                            false
                        } else {
                            true
                        }
                    });
                }
            }

            self.update_metrics(waiting.len());
        }
    }

    fn update_metrics(&self, waiting_len: usize) {
        self.metrics
            .pending_elements_certificate_waiter
            .with_label_values(&[&self.committee.epoch.to_string()])
            .set(self.pending.len() as i64);
        self.metrics
            .waiting_elements_certificate_waiter
            .with_label_values(&[&self.committee.epoch.to_string()])
            .set(waiting_len as i64);
    }
}
