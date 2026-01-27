// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
use shared_crypto::intent::Intent;

use crate::base_types::SuiAddress;
use crate::committee::EpochId;
use crate::digests::ZKLoginInputsDigest;
use crate::error::{SuiErrorKind, SuiResult};
use crate::signature::VerifyParams;
use crate::transaction::{SenderSignedData, TransactionDataAPI};
use lru::LruCache;
use parking_lot::RwLock;
use prometheus::IntCounter;
use std::hash::Hash;
use std::sync::Arc;

// Cache up to this many verified certs. We will need to tune this number in the future - a decent
// guess to start with is that it should be 10-20 times larger than peak transactions per second,
// on the assumption that we should see most certs twice within about 10-20 seconds at most:
// Once via RPC, once via consensus.
const VERIFIED_CERTIFICATE_CACHE_SIZE: usize = 100_000;

pub struct VerifiedDigestCache<D, V = ()> {
    inner: RwLock<LruCache<D, V>>,
    cache_hits_counter: IntCounter,
    cache_misses_counter: IntCounter,
    cache_evictions_counter: IntCounter,
}

impl<D: Hash + Eq + Copy, V: Clone> VerifiedDigestCache<D, V> {
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

    /// Returns the cached value for the given digest, if present.
    pub fn get_cached(&self, digest: &D) -> Option<V> {
        let inner = self.inner.read();
        if let Some(value) = inner.peek(digest) {
            self.cache_hits_counter.inc();
            Some(value.clone())
        } else {
            self.cache_misses_counter.inc();
            None
        }
    }

    pub fn cache_with_value(&self, digest: D, value: V) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.push(digest, value)
            && old.0 != digest
        {
            self.cache_evictions_counter.inc();
        }
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

impl<D: Hash + Eq + Copy> VerifiedDigestCache<D, ()> {
    pub fn cache_digest(&self, digest: D) {
        self.cache_with_value(digest, ())
    }

    pub fn cache_digests(&self, digests: Vec<D>) {
        let mut inner = self.inner.write();
        digests.into_iter().for_each(|d| {
            if let Some(old) = inner.push(d, ())
                && old.0 != d
            {
                self.cache_evictions_counter.inc();
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
}

/// Does crypto validation for a transaction which may be user-provided, or may be from a checkpoint.
/// Returns the signature index (into `tx_signatures`) used to verify each required signer,
/// in the same order as `required_signers`.
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    aliased_addresses: Vec<(SuiAddress, NonEmpty<SuiAddress>)>,
) -> SuiResult<Vec<u8>> {
    let intent_message = txn.intent_message();
    assert_eq!(intent_message.intent, Intent::sui_transaction());

    // 1. One signature per signer is required.
    let required_signers = txn.intent_message().value.required_signers();
    fp_ensure!(
        txn.inner().tx_signatures.len() == required_signers.len(),
        SuiErrorKind::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: required_signers.len()
        }
        .into()
    );

    // 2. System transactions do not require valid signatures. User-submitted transactions are
    // verified not to be system transactions before this point.
    if intent_message.value.is_system_tx() {
        // System tx are defined to use all of the dummy signatures provided.
        return Ok((0..required_signers.len() as u8).collect());
    }

    // 3. Each signer must provide a signature from one of the set of allowed aliases.
    // Use index mapping to track which signature index satisfies each required signer.
    let sig_mapping = txn.get_signer_sig_mapping(verify_params.verify_legacy_zklogin_address)?;

    let mut signer_to_sig_index = Vec::with_capacity(required_signers.len());
    for signer in required_signers.iter() {
        let alias_set = aliased_addresses
            .iter()
            .find(|(addr, _)| *addr == *signer)
            .map(|(_, aliases)| aliases.clone())
            .unwrap_or(NonEmpty::new(*signer));

        // Find the signature that matches any alias for this signer.
        let Some(sig_index) = alias_set
            .iter()
            .find_map(|alias| sig_mapping.get(alias).map(|(idx, _)| *idx))
        else {
            return Err(SuiErrorKind::SignerSignatureAbsent {
                expected: alias_set
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" or "),
                actual: sig_mapping.keys().map(|s| s.to_string()).collect(),
            }
            .into());
        };
        signer_to_sig_index.push(sig_index);
    }

    // 4. Every signature must be valid.
    for (signer, (_, signature)) in sig_mapping {
        signature.verify_authenticator(
            intent_message,
            signer,
            current_epoch,
            verify_params,
            zklogin_inputs_cache.clone(),
        )?;
    }
    Ok(signer_to_sig_index)
}
