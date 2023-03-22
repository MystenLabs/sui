// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use either::Either;
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

use mysten_metrics::monitored_scope;
use tap::TapFallible;
use tokio::runtime::Handle;
use tokio::{
    sync::oneshot,
    time::{timeout, Duration},
};
use tracing::error;
use sui_types::digests::Digest;
use sui_types::messages::SenderSignedData;

// Maximum amount of time we wait for a batch to fill up before verifying a partial batch.
const BATCH_TIMEOUT_MS: Duration = Duration::from_millis(10);

// Maximum size of batch to verify. Increasing this value will slightly improve CPU utilization
// (batching starts to hit steeply diminishing marginal returns around batch sizes of 16), at the
// cost of slightly increasing latency (BATCH_TIMEOUT_MS will be hit more frequently if system is
// not heavily loaded).
const MAX_BATCH_SIZE: usize = 8;

type Sender = oneshot::Sender<SuiResult<VerifiedCertificate>>;

/// Verifies signatures in ways that faster than verifying each signature individually.
/// - BLS signatures - caching and batch verification.
/// - Ed25519 signatures - caching.
///
/// Caching of verified digests is optimized for the happy flow:
/// - tx signatures - cached if verified directly, but not if verified as part of a tx cert (since
///   the tx cert is cached already).
/// - tx certs - cached when verified.
/// If a tx cert is cached already we don't need to verify its tx sigs since we never insert a tx
/// cert unless we verified its tx sigs.
///
pub struct SignatureVerifier {
    committee: Arc<Committee>,
    cache: VerifiedDigestCache,
    certs_queue: Mutex<CertBuffer>,
    pub metrics: Arc<SignatureVerifierMetrics>,
}

impl SignatureVerifier {
    pub fn new_with_batch_size(
        committee: Arc<Committee>,
        batch_size: usize,
        metrics: Arc<SignatureVerifierMetrics>,
    ) -> Self {
        Self {
            committee,
            cache: VerifiedDigestCache::new(metrics.clone()),
            certs_queue: Mutex::new(CertBuffer::new(batch_size)),
            metrics,
        }
    }

    pub fn new(committee: Arc<Committee>, metrics: Arc<SignatureVerifierMetrics>) -> Self {
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
            .filter(|cert| !self.cache.is_cached(&cert.full_message_digest()))
            .collect();

        // Note: this verifies also user sigs.
        batch_verify_all_certificates_and_checkpoints(
            &self.committee,
            &certs,
            &checkpoints,
            &self.cache,
        )
        .tap_ok(|_| {
            // Cache only tx certs, not tx signatures.
            self.cache.cache_verified_digests(
                certs
                    .into_iter()
                    .map(|cert| cert.full_message_digest())
                    .collect(),
            );
        })
    }

    /// Verifies one cert asynchronously, in a batch.
    pub async fn verify_cert(&self, cert: CertifiedTransaction) -> SuiResult<VerifiedCertificate> {
        let digest = cert.full_message_digest();
        if self.cache.is_cached(&digest) {
            return Ok(VerifiedCertificate::new_unchecked(cert));
        }
        self.verify_cert_skip_cache(cert)
            .await
            .tap_ok(|_| self.cache.cache_verified_digest(digest))
    }

    /// Exposed as a public method for the benchmarks
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

        self.verify_cert_inner(cert).await
    }

    async fn verify_cert_inner(
        &self,
        cert: CertifiedTransaction,
    ) -> SuiResult<VerifiedCertificate> {
        // Cancellation safety: we use parking_lot locks, which cannot be held across awaits.
        // Therefore once the queue has been taken by a thread, it is guaranteed to process the
        // queue and send all results before the future can be cancelled by the caller.
        let (tx, rx) = oneshot::channel();
        pin_mut!(rx);

        let prev_id_or_buffer = {
            let mut queue = self.queue.lock();
            queue.push(tx, cert);
            if queue.len() == queue.capacity() {
                Either::Right(CertBuffer::take_and_replace(queue))
            } else {
                Either::Left(queue.id)
            }
        };
        let prev_id = match prev_id_or_buffer {
            Either::Left(prev_id) => prev_id,
            Either::Right(buffer) => {
                self.metrics.full_batches.inc();
                self.process_queue(buffer).await;
                // unwrap ok - process_queue will have sent the result already
                return rx.try_recv().unwrap();
            }
        };

        if let Ok(res) = timeout(BATCH_TIMEOUT_MS, &mut rx).await {
            // unwrap ok - tx cannot have been dropped without sending a result.
            return res.unwrap();
        }
        self.metrics.timeouts.inc();

        let buffer = {
            let queue = self.queue.lock();
            // check if another thread took the queue while we were re-acquiring lock.
            if prev_id == queue.id {
                debug_assert_ne!(queue.len(), queue.capacity());
                Some(CertBuffer::take_and_replace(queue))
            } else {
                None
            }
        };

        if let Some(buffer) = buffer {
            self.metrics.partial_batches.inc();
            self.process_queue(buffer).await;
            // unwrap ok - process_queue will have sent the result already
            return rx.try_recv().unwrap();
        }

        // unwrap ok - another thread took the queue while we were re-acquiring the lock and is
        // guaranteed to process the queue immediately.
        rx.await.unwrap()
    }

    async fn process_queue(&self, buffer: CertBuffer) {
        let committee = self.committee.clone();
        let metrics = self.metrics.clone();
        Handle::current()
            .spawn_blocking(move || Self::process_queue_sync(committee, metrics, buffer))
            .await
            .expect("Spawn blocking should not fail");
    }

    fn process_queue_sync(
        committee: Arc<Committee>,
        metrics: Arc<BatchCertificateVerifierMetrics>,
        buffer: CertBuffer,
    ) {
        let _scope = monitored_scope("BatchCertificateVerifier::process_queue");

        let results = batch_verify_certificates(&committee, &buffer.certs);
        izip!(
            results.into_iter(),
            buffer.certs.into_iter(),
            buffer.senders.into_iter(),
        )
        .for_each(|(result, cert, tx)| {
            tx.send(match result {
                Ok(()) => {
                    metrics.total_verified_certs.inc();
                    Ok(VerifiedCertificate::new_unchecked(cert))
                }
                Err(e) => {
                    metrics.total_failed_certs.inc();
                    Err(e)
                }
            })
            .ok();
        });
    }

    pub fn verify_tx(&self, signed_tx: &SenderSignedData) -> SuiResult {
        self.cache
            .is_verified(signed_tx.full_message_digest(), || signed_tx.verify(None))
    }
}

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

pub struct SignatureVerifierMetrics {
    digests_cache_hits: IntCounter,
    digests_cache_evictions: IntCounter,
    timeouts: IntCounter,
    full_batches: IntCounter,
    partial_batches: IntCounter,
    total_verified_certs: IntCounter,
    total_failed_certs: IntCounter,
}

impl SignatureVerifierMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            digests_cache_hits: register_int_counter_with_registry!(
                "digests_cache_hits",
                "Number of digests which were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            digests_cache_evictions: register_int_counter_with_registry!(
                "digests_cache_evictions",
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
    cache: &VerifiedDigestCache,
) -> SuiResult {
    for cert in certs {
        // We don't cache the tx sig in this case since we cache the tx cert.
        let tx = cert.data();
        if !cache.is_cached(&tx.full_message_digest()) {
            tx.verify(None)?;
        }
    }
    for ckpt in checkpoints {
        ckpt.data().verify(Some(committee.epoch()))?;
    }

    batch_verify(committee, certs, checkpoints)
}

/// Verifies certificates in batch mode, but returns a separate result for each cert.
pub fn batch_verify_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
    cache: &VerifiedDigestCache,
) -> Vec<SuiResult> {
    match batch_verify(committee, certs, &[]) {
        Ok(_) => certs
            .iter()
            // Verify the user signature without caching it since the tx cert is cached.
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
            // TODO: verify_signature checks again the user signature.
            .map(|c| cache.is_verified(c.full_message_digest(), || c.verify_signature(committee)))
            .collect(),

        Err(e) => vec![Err(e)],
    }
}

fn batch_verify(
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

// Cache up to 40000 verified digests. We will need to tune this number in the future - a decent
// guess to start with is that it should be 10-20 times larger than peak transactions per second,
// on the assumption that for each transaction we should see the same cert and user signature within
// about 10-20 seconds at most: Once via RPC, once via consensus.
const VERIFIED_DIGEST_CACHE_SIZE: usize = 40000;

pub struct VerifiedDigestCache {
    inner: RwLock<LruCache<Digest, ()>>,
    metrics: Arc<SignatureVerifierMetrics>,
}

impl VerifiedDigestCache {
    pub fn new(metrics: Arc<SignatureVerifierMetrics>) -> Self {
        Self {
            inner: RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(VERIFIED_DIGEST_CACHE_SIZE).unwrap(),
            )),
            metrics,
        }
    }

    pub fn is_cached(&self, digest: &Digest) -> bool {
        let inner = self.inner.read();
        if inner.contains(digest) {
            self.metrics.digests_cache_hits.inc();
            true
        } else {
            false
        }
    }

    pub fn cache_verified_digest(&self, digest: Digest) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.push(digest, ()) {
            if old.0 != digest {
                self.metrics.digests_cache_evictions.inc();
            }
        }
    }

    pub fn cache_verified_digests(&self, digests: Vec<Digest>) {
        let mut inner = self.inner.write();
        digests.into_iter().for_each(|d| {
            if let Some(old) = inner.push(d, ()) {
                if old.0 != d {
                    self.metrics.digests_cache_evictions.inc();
                }
            }
        });
    }

    pub fn is_verified<F>(&self, digest: Digest, verify_callback: F) -> SuiResult
    where
        F: FnOnce() -> SuiResult,
    {
        if !self.is_cached(&digest) {
            verify_callback()?;
            self.cache_verified_digest(digest);
        }
        Ok(())
    }
}
