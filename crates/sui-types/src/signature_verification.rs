// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
use shared_crypto::intent::Intent;

use crate::committee::EpochId;
use crate::digests::ZKLoginInputsDigest;
use crate::error::{SuiError, SuiResult};
use crate::signature::VerifyParams;
use crate::transaction::{SenderSignedData, TransactionDataAPI};
use lru::LruCache;
use parking_lot::RwLock;
use prometheus::IntCounter;
use std::hash::Hash;
use std::sync::Arc;

// Cache up to 20000 verified certs. We will need to tune this number in the future - a decent
// guess to start with is that it should be 10-20 times larger than peak transactions per second,
// on the assumption that we should see most certs twice within about 10-20 seconds at most: Once via RPC, once via consensus.
const VERIFIED_CERTIFICATE_CACHE_SIZE: usize = 20000;

pub struct VerifiedDigestCache<D> {
    inner: RwLock<LruCache<D, ()>>,
    cache_hits_counter: IntCounter,
    cache_misses_counter: IntCounter,
    cache_evictions_counter: IntCounter,
}

impl<D: Hash + Eq + Copy> VerifiedDigestCache<D> {
    pub fn new(
        cache_hits_counter: IntCounter,
        cache_misses_counter: IntCounter,
        cache_evictions_counter: IntCounter,
    ) -> Self {
        Self {
            inner: RwLock::new(LruCache::new(
                std::num::NonZeroUsize::new(VERIFIED_CERTIFICATE_CACHE_SIZE).unwrap(),
            )),
            cache_hits_counter,
            cache_misses_counter,
            cache_evictions_counter,
        }
    }

    pub fn is_cached(&self, digest: &D) -> bool {
        let inner = self.inner.read();
        if inner.contains(digest) {
            self.cache_hits_counter.inc();
            true
        } else {
            self.cache_misses_counter.inc();
            false
        }
    }

    pub fn cache_digest(&self, digest: D) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.push(digest, ()) {
            if old.0 != digest {
                self.cache_evictions_counter.inc();
            }
        }
    }

    pub fn cache_digests(&self, digests: Vec<D>) {
        let mut inner = self.inner.write();
        digests.into_iter().for_each(|d| {
            if let Some(old) = inner.push(d, ()) {
                if old.0 != d {
                    self.cache_evictions_counter.inc();
                }
            }
        });
    }

    pub fn is_verified<F, G>(&self, digest: D, verify_callback: F, uncached_checks: G) -> SuiResult
    where
        F: FnOnce() -> SuiResult,
        G: FnOnce() -> SuiResult,
    {
        if !self.is_cached(&digest) {
            verify_callback()?;
            self.cache_digest(digest);
        } else {
            // Checks that are required to be performed outside the cache.
            uncached_checks()?;
        }
        Ok(())
    }

    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.clear();
    }

    // Initialize an empty cache when the cache is not needed (in testing scenarios, graphql and rosetta initialization).
    pub fn new_empty() -> Self {
        Self::new(
            IntCounter::new("test_cache_hits", "test cache hits").unwrap(),
            IntCounter::new("test_cache_misses", "test cache misses").unwrap(),
            IntCounter::new("test_cache_evictions", "test cache evictions").unwrap(),
        )
    }
}

/// Does crypto validation for a transaction which may be user-provided, or may be from a checkpoint.
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
) -> SuiResult {
    let intent_message = txn.intent_message();
    assert_eq!(intent_message.intent, Intent::sui_transaction());

    // 1. System transactions do not require signatures. User-submitted transactions are verified not to
    // be system transactions before this point
    if intent_message.value.is_system_tx() {
        return Ok(());
    }

    // 2. One signature per signer is required.
    let signers: NonEmpty<_> = txn.intent_message().value.signers();
    fp_ensure!(
        txn.inner().tx_signatures.len() == signers.len(),
        SuiError::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: signers.len()
        }
    );

    // 3. Each signer must provide a signature.
    let present_sigs = txn.get_signer_sig_mapping(verify_params.verify_legacy_zklogin_address)?;
    for s in signers {
        if !present_sigs.contains_key(&s) {
            return Err(SuiError::SignerSignatureAbsent {
                expected: s.to_string(),
                actual: present_sigs.keys().map(|s| s.to_string()).collect(),
            });
        }
    }

    // 4. Every signature must be valid.
    for (signer, signature) in present_sigs {
        signature.verify_authenticator(
            intent_message,
            signer,
            current_epoch,
            verify_params,
            zklogin_inputs_cache.clone(),
        )?;
    }
    Ok(())
}
