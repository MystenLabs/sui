// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::PrimaryMetrics;
use config::Committee;
use crypto::traits::VerifyingKey;
use dashmap::DashMap;
use futures::{
    future::try_join_all,
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
};
use once_cell::sync::OnceCell;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, watch,
    },
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::error;
use types::{
    error::{DagError, DagResult},
    Certificate, CertificateDigest, HeaderDigest, ReconfigureNotification, Round,
};

#[cfg(test)]
#[path = "tests/certificate_waiter_tests.rs"]
pub mod certificate_waiter_tests;

/// The resolution of the timer that checks whether we received replies to our parent requests, and triggers
/// a round of GC if we didn't.
const GC_RESOLUTION: u64 = 10_000;

/// Waits to receive all the ancestors of a certificate before looping it back to the `Core`
/// for further processing.
pub struct CertificateWaiter<PublicKey: VerifyingKey> {
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The persistent storage.
    store: Store<CertificateDigest, Certificate<PublicKey>>,
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Watch channel notifying of epoch changes, it is only used for cleanup.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Receives sync commands from the `Synchronizer`.
    rx_synchronizer: Receiver<Certificate<PublicKey>>,
    /// Loops back to the core certificates for which we got all parents.
    tx_core: Sender<Certificate<PublicKey>>,
    /// List of digests (certificates) that are waiting to be processed. Their processing will
    /// resume when we get all their dependencies. The map holds a cancellation `Sender`
    /// which we can use to give up on a certificate.
    // TODO: remove the OnceCell once drain_filter stabilizes
    pending: DashMap<HeaderDigest, (Round, OnceCell<oneshot::Sender<()>>)>,
    /// The metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl<PublicKey: VerifyingKey> CertificateWaiter<PublicKey> {
    pub fn spawn(
        committee: Committee<PublicKey>,
        store: Store<CertificateDigest, Certificate<PublicKey>>,
        consensus_round: Arc<AtomicU64>,
        gc_depth: Round,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_synchronizer: Receiver<Certificate<PublicKey>>,
        tx_core: Sender<Certificate<PublicKey>>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                committee,
                store,
                consensus_round,
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
        store: &Store<CertificateDigest, Certificate<PublicKey>>,
        deliver: Certificate<PublicKey>,
        cancel_handle: oneshot::Receiver<()>,
    ) -> DagResult<Certificate<PublicKey>> {
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
        let mut waiting = FuturesUnordered::new();

        let timer = sleep(Duration::from_millis(GC_RESOLUTION));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                Some(certificate) = self.rx_synchronizer.recv() => {
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
                    waiting.push(fut);
                }
                Some(result) = waiting.next() => match result {
                    Ok(certificate) => {
                        // TODO [issue #115]: To ensure crash-recovery of consensus, it is not enough to send every
                        // certificate for which their ancestors are in the storage. After recovery, we may also
                        // need to send a all parents certificates with rounds greater then `last_committed`.

                        self.tx_core.send(certificate).await.expect("Failed to send certificate");
                    },
                    Err(e) => {
                        error!("{e}");
                        panic!("Storage failure: killing node.");
                    }
                },
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow_and_update().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(committee) => {
                            self.committee = committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                            self.pending.clear();
                        },
                        ReconfigureNotification::Shutdown => return
                    }

                }
                () = &mut timer => {
                    // We still would like to GC even if we do not receive anything from either the synchronizer or
                    //  the Waiter. This can happen, as we may have asked for certificates that (at the time of ask)
                    // were not past our GC bound, but which round swept up past our GC bound as we advanced rounds
                    // & caught up with the network.
                    // Those certificates may get stuck in pending, unless we periodically clean up.

                    // Reschedule the timer.
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(GC_RESOLUTION));
                },
            }

            // Cleanup internal state. Deliver the certificates waiting on garbage collected ancestors.
            let round = self.consensus_round.load(Ordering::Relaxed);
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

            self.update_metrics();
        }
    }

    fn update_metrics(&self) {
        self.metrics
            .pending_elements_certificate_waiter
            .with_label_values(&[&self.committee.epoch.to_string()])
            .set(self.pending.len() as i64);
    }
}
