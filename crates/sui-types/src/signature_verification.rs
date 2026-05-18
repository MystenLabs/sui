// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_types_verified::signature_verification::{SigVerifyError, SignatureVerifiable};

use crate::base_types::SuiAddress;
use crate::committee::EpochId;
use crate::digests::ZKLoginInputsDigest;
use crate::error::{SuiError, SuiErrorKind, SuiResult};
use crate::signature::{GenericSignature, VerifyParams};
use crate::transaction::{SenderSignedData, TransactionData, TransactionDataAPI};
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

/// A `GenericSignature` bundled with the context needed to verify it.
///
/// This wrapper implements `SignatureVerifiable<SuiAddress>` so that the
/// generic verified function has no dependency on `sui-types` concrete types.
pub struct VerifiableSig<'a> {
    sig: &'a GenericSignature,
    intent_message: &'a IntentMessage<TransactionData>,
    verify_params: &'a VerifyParams,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
}

impl<'a> SignatureVerifiable<SuiAddress> for VerifiableSig<'a> {
    fn try_derive_addresses(&self) -> Result<Vec<SuiAddress>, SigVerifyError> {
        let mut addrs = Vec::new();
        // A zklogin signature with legacy-address support yields two addresses.
        if self.verify_params.verify_legacy_zklogin_address {
            if let GenericSignature::ZkLoginAuthenticator(z) = self.sig {
                addrs.push(
                    SuiAddress::try_from_padded(&z.inputs)
                        .map_err(|_| SigVerifyError::AddressDerivationFailed)?,
                );
            }
        }
        let canonical =
            SuiAddress::try_from(self.sig).map_err(|_| SigVerifyError::AddressDerivationFailed)?;
        addrs.push(canonical);
        Ok(addrs)
    }

    fn verify_for_address(&self, addr: &SuiAddress, epoch: u64) -> Result<(), SigVerifyError> {
        self.sig
            .verify_authenticator(
                self.intent_message,
                *addr,
                epoch,
                self.verify_params,
                self.zklogin_inputs_cache.clone(),
            )
            .map_err(|_| SigVerifyError::CryptoVerificationFailed)
    }
}

fn sig_verify_err_to_sui(e: SigVerifyError) -> SuiError {
    match e {
        SigVerifyError::SignerCountMismatch { actual, expected } => {
            SuiErrorKind::SignerSignatureNumberMismatch { actual, expected }.into()
        }
        SigVerifyError::AddressDerivationFailed => SuiErrorKind::InvalidSignature {
            error: "address derivation failed".to_owned(),
        }
        .into(),
        SigVerifyError::SignerAbsent => SuiErrorKind::SignerSignatureAbsent {
            expected: String::new(),
            actual: vec![],
        }
        .into(),
        SigVerifyError::CryptoVerificationFailed => SuiErrorKind::InvalidSignature {
            error: "cryptographic verification failed".to_owned(),
        }
        .into(),
    }
}

/// Crypto validation for a sender-signed transaction.
///
/// Returns the signature index (into `tx_signatures`) used to verify each
/// required signer, in the same order as `required_signers`.
///
/// Handles intent checking and the system-transaction bypass, then delegates
/// user-transaction verification to the Verus-verified
/// [`sui_types_verified::signature_verification::verify_signatures`].
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    aliased_addresses: Vec<(SuiAddress, NonEmpty<SuiAddress>)>,
) -> SuiResult<Vec<u8>> {
    let intent_message = txn.intent_message();

    // Intent check: must be a Sui transaction.
    if intent_message.intent != Intent::sui_transaction() {
        return Err(SuiErrorKind::InvalidSignature {
            error: "wrong intent".to_owned(),
        }
        .into());
    }

    let required_signers = sui_types_verified::signature_verification::build_required_signers(
        &intent_message.value.required_signers(),
        &aliased_addresses,
    );

    // System transactions are unconditionally valid; return sequential indices.
    if intent_message.value.is_system_tx() {
        return Ok((0..required_signers.len() as u8).collect());
    }

    // Precondition required by the verified function: signer count fits in u8.
    // In practice required_signers has at most 2 elements.
    assert!(required_signers.len() <= u8::MAX as usize);

    let verifiable_sigs: Vec<VerifiableSig> = txn
        .tx_signatures()
        .iter()
        .map(|sig| VerifiableSig {
            sig,
            intent_message,
            verify_params,
            zklogin_inputs_cache: zklogin_inputs_cache.clone(),
        })
        .collect();

    sui_types_verified::signature_verification::verify_signatures(
        &verifiable_sigs,
        &required_signers,
        current_epoch,
    )
    .map_err(sig_verify_err_to_sui)
}
