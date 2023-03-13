// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::pin_mut;
use itertools::izip;
use lru::LruCache;
use parking_lot::{Mutex, MutexGuard, RwLock};
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use shared_crypto::intent::Intent;
use std::sync::Arc;
use sui_types::{
    committee::Committee,
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    digests::CertificateDigest,
    error::{SuiError, SuiResult},
    message_envelope::Message,
    messages::{CertifiedTransaction, VerifiedCertificate},
    messages_checkpoint::SignedCheckpointSummary,
};

use tap::TapFallible;
use tokio::{
    sync::oneshot,
    time::{timeout, Duration},
};
use tracing::error;

// Maximum amount of time we wait for a batch to fill up before verifying a partial batch.
const BATCH_TIMEOUT_MS: Duration = Duration::from_millis(10);

// Maximum size of batch to verify. Increasing this value will slightly improve CPU utilization
// (batching starts to hit steeply diminishing marginal returns around batch sizes of 16), at the
// cost of slightly increasing latency (BATCH_TIMEOUT_MS will be hit more frequently if system is
// not heavily loaded).
const MAX_BATCH_SIZE: usize = 8;

type Sender = oneshot::Sender<SuiResult<VerifiedCertificate>>;

struct CertBuffer {
    certs: Vec<CertifiedTransaction>,
    senders: Vec<Sender>,
    id: u64,
}

impl CertBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            certs: Vec::with_capacity(capacity),
            senders: Vec::with_capacity(capacity),
            id: 0,
        }
    }

    fn take_and_replace(&mut self) -> Self {
        let mut new = CertBuffer::new(self.capacity());
        new.id = self.id + 1;
        std::mem::swap(&mut new, self);
        new
    }

    fn capacity(&self) -> usize {
        debug_assert_eq!(self.certs.capacity(), self.senders.capacity());
        self.certs.capacity()
    }

    fn len(&self) -> usize {
        debug_assert_eq!(self.certs.len(), self.senders.len());
        self.certs.len()
    }

    fn push(&mut self, tx: Sender, cert: CertifiedTransaction) {
        self.senders.push(tx);
        self.certs.push(cert);
    }
}

pub struct BatchCertificateVerifier {
    committee: Arc<Committee>,
    cache: VerifiedCertificateCache,

    queue: Mutex<CertBuffer>,
    pub metrics: Arc<BatchCertificateVerifierMetrics>,
}

impl BatchCertificateVerifier {
    pub fn new_with_batch_size(
        committee: Arc<Committee>,
        batch_size: usize,
        metrics: Arc<BatchCertificateVerifierMetrics>,
    ) -> Self {
        Self {
            committee,
            cache: VerifiedCertificateCache::new(metrics.clone()),
            queue: Mutex::new(CertBuffer::new(batch_size)),
            metrics,
        }
    }

    pub fn new(committee: Arc<Committee>, metrics: Arc<BatchCertificateVerifierMetrics>) -> Self {
        Self::new_with_batch_size(committee, MAX_BATCH_SIZE, metrics)
    }

    /// Verifies all certs, returns Ok only if all are valid.
    pub fn verify_certs_and_checkpoints(
        &self,
        certs: Vec<CertifiedTransaction>,
        checkpoints: Vec<SignedCheckpointSummary>,
    ) -> SuiResult {
        let certs: Vec<_> = certs
            .into_iter()
            .filter(|cert| !self.cache.is_cert_verified(&cert.certificate_digest()))
            .collect();

        // Note: this verifies user sigs
        batch_verify_all_certificates_and_checkpoints(&self.committee, &certs, &checkpoints).tap_ok(
            |_| {
                self.cache.cache_certs_verified(
                    certs.into_iter().map(|c| c.certificate_digest()).collect(),
                )
            },
        )
    }

    /// Verifies one cert asynchronously, in a batch.
    pub async fn verify_cert(&self, cert: CertifiedTransaction) -> SuiResult<VerifiedCertificate> {
        let cert_digest = cert.certificate_digest();
        if self.cache.is_cert_verified(&cert_digest) {
            return Ok(VerifiedCertificate::new_unchecked(cert));
        }
        self.verify_cert_skip_cache(cert)
            .await
            .tap_ok(|_| self.cache.cache_cert_verified(cert_digest))
    }

    /// exposed as a public method for the benchmarks
    pub async fn verify_cert_skip_cache(
        &self,
        cert: CertifiedTransaction,
    ) -> SuiResult<VerifiedCertificate> {
        // this is the only innocent error we are likely to encounter - filter it before we poison
        // a whole batch.
        if cert.auth_sig().epoch != self.committee.epoch() {
            return Err(SuiError::WrongEpoch {
                expected_epoch: self.committee.epoch(),
                actual_epoch: cert.auth_sig().epoch,
            });
        }

        cert.verify_sender_signatures()?;
        self.verify_cert_inner(cert).await
    }

    async fn verify_cert_inner(
        &self,
        cert: CertifiedTransaction,
    ) -> SuiResult<VerifiedCertificate> {
        let (tx, rx) = oneshot::channel();
        pin_mut!(rx);

        let prev_id = {
            let mut queue = self.queue.lock();
            queue.push(tx, cert);
            if queue.len() == queue.capacity() {
                self.metrics.full_batches.inc();
                self.process_queue(queue);
                // unwrap ok - process_queue will have sent the result already
                return rx.try_recv().unwrap();
            }
            queue.id
        };

        if let Ok(res) = timeout(BATCH_TIMEOUT_MS, &mut rx).await {
            // unwrap ok - tx cannot have been dropped without sending a result.
            return res.unwrap();
        }
        self.metrics.timeouts.inc();

        {
            let queue = self.queue.lock();
            // check if another thread took the queue while we were re-acquiring lock.
            if prev_id == queue.id {
                debug_assert_ne!(queue.len(), queue.capacity());
                self.metrics.partial_batches.inc();
                self.process_queue(queue);
                // unwrap ok - process_queue will have sent the result already
                return rx.try_recv().unwrap();
            }
        }

        // unwrap ok - another thread took the queue while we were re-acquiring the lock and is
        // guaranteed to process the queue immediately.
        rx.await.unwrap()
    }

    fn process_queue(&self, mut queue: MutexGuard<'_, CertBuffer>) {
        let taken = queue.take_and_replace();
        drop(queue);

        let results = batch_verify_certificates(&self.committee, &taken.certs);
        izip!(
            results.into_iter(),
            taken.certs.into_iter(),
            taken.senders.into_iter(),
        )
        .for_each(|(result, cert, tx)| {
            tx.send(match result {
                Ok(()) => {
                    self.metrics.total_verified_certs.inc();
                    Ok(VerifiedCertificate::new_unchecked(cert))
                }
                Err(e) => {
                    self.metrics.total_failed_certs.inc();
                    Err(e)
                }
            })
            .ok();
        });
    }
}

pub struct BatchCertificateVerifierMetrics {
    certificate_signatures_cache_hits: IntCounter,
    certificate_signatures_cache_evictions: IntCounter,
    timeouts: IntCounter,
    full_batches: IntCounter,
    partial_batches: IntCounter,
    total_verified_certs: IntCounter,
    total_failed_certs: IntCounter,
}

impl BatchCertificateVerifierMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            certificate_signatures_cache_hits: register_int_counter_with_registry!(
                "certificate_signatures_cache_hits",
                "Number of certificates which were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            certificate_signatures_cache_evictions: register_int_counter_with_registry!(
                "certificate_signatures_cache_evictions",
                "Number of times we evict a pre-existing key were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            timeouts: register_int_counter_with_registry!(
                "async_batch_verifier_timeouts",
                "Number of times batch verifier times out and verifies a partial batch",
                registry
            )
            .unwrap(),
            full_batches: register_int_counter_with_registry!(
                "async_batch_verifier_full_batches",
                "Number of times batch verifier verifies a full batch",
                registry
            )
            .unwrap(),
            partial_batches: register_int_counter_with_registry!(
                "async_batch_verifier_partial_batches",
                "Number of times batch verifier verifies a partial batch",
                registry
            )
            .unwrap(),
            total_verified_certs: register_int_counter_with_registry!(
                "async_batch_verifier_total_verified_certs",
                "Total number of certs batch verifier has verified",
                registry
            )
            .unwrap(),
            total_failed_certs: register_int_counter_with_registry!(
                "async_batch_verifier_total_failed_certs",
                "Total number of certs batch verifier has rejected",
                registry
            )
            .unwrap(),
        })
    }
}

/// Verifies all certificates - if any fail return error.
pub fn batch_verify_all_certificates_and_checkpoints(
    committee: &Committee,
    certs: &[CertifiedTransaction],
    checkpoints: &[SignedCheckpointSummary],
) -> SuiResult {
    for cert in certs {
        cert.verify_sender_signatures()?;
    }
    for ckpt in checkpoints {
        ckpt.data().verify(Some(committee.epoch()))?;
    }

    batch_verify_certificates_impl(committee, certs, checkpoints)
}

/// Verifies certificates in batch mode, but returns a separate result for each cert.
pub fn batch_verify_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> Vec<SuiResult> {
    match batch_verify_certificates_impl(committee, certs, &[]) {
        Ok(_) => certs
            .iter()
            .map(|c| {
                c.data().verify(None).tap_err(|e| {
                    error!(
                        "Cert was signed by quorum, but contained bad user signatures! {}",
                        e
                    )
                })?;
                Ok(())
            })
            .collect(),

        // Verify one by one to find which certs were invalid.
        Err(_) if certs.len() > 1 => certs
            .iter()
            .map(|c| c.verify_signature(committee))
            .collect(),

        Err(e) => vec![Err(e)],
    }
}

fn batch_verify_certificates_impl(
    committee: &Committee,
    certs: &[CertifiedTransaction],
    checkpoints: &[SignedCheckpointSummary],
) -> SuiResult {
    let mut obligation = VerificationObligation::default();

    for cert in certs {
        let idx = obligation.add_message(
            cert.data(),
            cert.epoch(),
            Intent::default().with_scope(cert.scope()),
        );
        cert.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    for ckpt in checkpoints {
        let idx = obligation.add_message(
            ckpt.data(),
            ckpt.epoch(),
            Intent::default().with_scope(ckpt.scope()),
        );
        ckpt.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    obligation.verify_all()
}

// Cache up to 20000 verified certs. We will need to tune this number in the future - a decent
// guess to start with is that it should be 10-20 times larger than peak transactions per second,
// on the assumption that we should see most certs twice within about 10-20 seconds at most: Once via RPC, once via consensus.
const VERIFIED_CERTIFICATE_CACHE_SIZE: usize = 20000;

pub struct VerifiedCertificateCache {
    inner: RwLock<LruCache<CertificateDigest, ()>>,
    metrics: Arc<BatchCertificateVerifierMetrics>,
}

impl VerifiedCertificateCache {
    pub fn new(metrics: Arc<BatchCertificateVerifierMetrics>) -> Self {
        Self {
            inner: RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(VERIFIED_CERTIFICATE_CACHE_SIZE).unwrap(),
            )),
            metrics,
        }
    }

    pub fn is_cert_verified(&self, digest: &CertificateDigest) -> bool {
        let inner = self.inner.read();
        if inner.contains(digest) {
            self.metrics.certificate_signatures_cache_hits.inc();
            true
        } else {
            false
        }
    }

    pub fn cache_cert_verified(&self, digest: CertificateDigest) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.push(digest, ()) {
            if old.0 != digest {
                self.metrics.certificate_signatures_cache_evictions.inc();
            }
        }
    }

    pub fn cache_certs_verified(&self, digests: Vec<CertificateDigest>) {
        let mut inner = self.inner.write();
        digests.into_iter().for_each(|d| {
            if let Some(old) = inner.push(d, ()) {
                if old.0 != d {
                    self.metrics.certificate_signatures_cache_evictions.inc();
                }
            }
        });
    }
}
