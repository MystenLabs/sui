// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::pin_mut;
use itertools::izip;
use parking_lot::{Mutex, MutexGuard};
use shared_crypto::intent::{Intent, IntentScope};
use std::sync::Arc;
use sui_types::{
    committee::Committee,
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    error::{SuiError, SuiResult},
    message_envelope::Message,
    messages::{CertifiedTransaction, VerifiedCertificate},
};

use tap::TapFallible;
use tokio::{
    sync::oneshot,
    time::{timeout, Duration},
};
use tracing::error;

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

pub struct AsyncBatchVerifier {
    committee: Arc<Committee>,
    queue: Mutex<CertBuffer>,
}

impl AsyncBatchVerifier {
    pub fn new(committee: Arc<Committee>, capacity: usize) -> Self {
        Self {
            committee,
            queue: Mutex::new(CertBuffer::new(capacity)),
        }
    }

    pub async fn verify_cert(&self, cert: CertifiedTransaction) -> SuiResult<VerifiedCertificate> {
        // this is the only innocent error we are likely to encounter - filter it before we poison
        // a whole batch.
        if cert.auth_sig().epoch != self.committee.epoch() {
            return Err(SuiError::WrongEpoch {
                expected_epoch: self.committee.epoch(),
                actual_epoch: cert.auth_sig().epoch,
            });
        }

        let (tx, rx) = oneshot::channel();
        pin_mut!(rx);

        let prev_id = {
            let mut queue = self.queue.lock();
            queue.push(tx, cert);
            if queue.len() == queue.capacity() {
                self.process_queue(queue);
                // unwrap ok - process_queue will have sent the result already
                return rx.try_recv().unwrap();
            }
            queue.id
        };

        if let Ok(res) = timeout(Duration::from_millis(10), &mut rx).await {
            // unwrap ok - tx cannot have been dropped without sending a result.
            return res.unwrap();
        }

        {
            let queue = self.queue.lock();
            // check if another thread took the queue while we were re-acquiring lock.
            if prev_id == queue.id {
                self.process_queue(queue);
                // unwrap ok - process_queue will have sent the result already
                return rx.try_recv().unwrap();
            }
        }

        // unwrap ok - another took the queue while we were re-acquiring the lock and is
        // guaranteed to process the queue immediately.
        return rx.await.unwrap();
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
                Ok(()) => Ok(VerifiedCertificate::new_unchecked(cert)),
                Err(e) => Err(e),
            })
            .ok();
        });
    }
}

/// Verifies all certificates - if any fail return error.
pub fn batch_verify_all_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> SuiResult {
    // Verify user signatures
    for cert in certs {
        cert.data().verify(None)?;
    }

    batch_verify_certificates_impl(committee, certs)
}

/// Verifies certificates in batch mode, but returns a separate result for each cert.
pub fn batch_verify_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> Vec<SuiResult> {
    match batch_verify_certificates_impl(committee, certs) {
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
) -> SuiResult {
    let mut obligation = VerificationObligation::default();

    for cert in certs {
        let idx = obligation.add_message(
            cert.data(),
            cert.epoch(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
        );
        cert.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    obligation.verify_all()
}
