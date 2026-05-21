// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
use shared_crypto::intent::Intent;

use crate::base_types::SuiAddress;
use crate::committee::EpochId;
use crate::digests::ZKLoginInputsDigest;
use crate::error::{SuiError, SuiErrorKind, SuiResult};
use crate::signature::{GenericSignature, VerifyParams};
use crate::transaction::{SenderSignedData, TransactionDataAPI};
use lru::LruCache;
use parking_lot::RwLock;
use prometheus::IntCounter;
use std::collections::{BTreeMap, BTreeSet};
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
///
/// When `fix_aliased_signer_signatures` is set, the verifier requires a one-to-one
/// matching between required signers and provided signatures: each required signer
/// is matched to exactly one signature whose recovered address is one of the
/// signer's allowed aliases, and every signature is matched to exactly one signer.
/// When unset, it uses the legacy behavior, which assigns the first matching
/// signature to each signer without checking that the assignment is one-to-one.
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    aliased_addresses: Vec<(SuiAddress, NonEmpty<SuiAddress>)>,
    fix_aliased_signer_signatures: bool,
) -> SuiResult<Vec<u8>> {
    let intent_message = txn.intent_message();
    assert_eq!(intent_message.intent, Intent::sui_transaction());

    let required_signers = txn.intent_message().value.required_signers();
    fp_ensure!(
        txn.inner().tx_signatures.len() == required_signers.len(),
        SuiErrorKind::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: required_signers.len()
        }
        .into()
    );

    // System transactions do not require valid signatures. User-submitted transactions are
    // verified not to be system transactions before this point.
    if intent_message.value.is_system_tx() {
        // System tx are defined to use all of the dummy signatures provided.
        return Ok((0..required_signers.len() as u8).collect());
    }

    let sig_mapping = txn.get_signer_sig_mapping(verify_params.verify_legacy_zklogin_address)?;

    let signer_to_sig_index = if fix_aliased_signer_signatures {
        match_signers_to_signatures(&required_signers, &aliased_addresses, &sig_mapping)?
    } else {
        greedy_match_signers_to_signatures(&required_signers, &aliased_addresses, &sig_mapping)?
    };

    // Every signature must be valid.
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

/// The set of addresses allowed to sign for `signer`: its alias set if it has one,
/// otherwise just `signer` itself.
fn resolve_alias_set(
    aliased_addresses: &[(SuiAddress, NonEmpty<SuiAddress>)],
    signer: &SuiAddress,
) -> NonEmpty<SuiAddress> {
    aliased_addresses
        .iter()
        .find(|(addr, _)| addr == signer)
        .map(|(_, aliases)| aliases.clone())
        .unwrap_or(NonEmpty::new(*signer))
}

/// Builds the error returned when a required signer cannot be bound to a signature.
fn signer_signature_absent(
    alias_set: &NonEmpty<SuiAddress>,
    sig_mapping: &BTreeMap<SuiAddress, (u8, &GenericSignature)>,
) -> SuiError {
    SuiErrorKind::SignerSignatureAbsent {
        expected: alias_set
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(" or "),
        actual: sig_mapping.keys().map(|s| s.to_string()).collect(),
    }
    .into()
}

/// Legacy signer-to-signature assignment: greedily binds each required signer to
/// the first signature reachable through its alias set, without checking that the
/// assignment is one-to-one.
fn greedy_match_signers_to_signatures(
    required_signers: &NonEmpty<SuiAddress>,
    aliased_addresses: &[(SuiAddress, NonEmpty<SuiAddress>)],
    sig_mapping: &BTreeMap<SuiAddress, (u8, &GenericSignature)>,
) -> SuiResult<Vec<u8>> {
    let mut signer_to_sig_index = Vec::with_capacity(required_signers.len());
    for signer in required_signers.iter() {
        let alias_set = resolve_alias_set(aliased_addresses, signer);
        let Some(sig_index) = alias_set
            .iter()
            .find_map(|alias| sig_mapping.get(alias).map(|(idx, _)| *idx))
        else {
            return Err(signer_signature_absent(&alias_set, sig_mapping));
        };
        signer_to_sig_index.push(sig_index);
    }
    Ok(signer_to_sig_index)
}

/// Finds a one-to-one matching between required signers and the signatures in
/// `sig_mapping`: every required signer is matched to exactly one signature whose
/// recovered address is one of the signer's allowed aliases, and no signature is
/// matched to more than one signer. Returns the matched signature index for each
/// required signer, in `required_signers` order.
///
/// The caller has already checked that the signature and required-signer counts
/// are equal, so a successful matching is necessarily perfect: every signature is
/// also bound to exactly one required signer.
fn match_signers_to_signatures(
    required_signers: &NonEmpty<SuiAddress>,
    aliased_addresses: &[(SuiAddress, NonEmpty<SuiAddress>)],
    sig_mapping: &BTreeMap<SuiAddress, (u8, &GenericSignature)>,
) -> SuiResult<Vec<u8>> {
    let alias_sets: Vec<NonEmpty<SuiAddress>> = required_signers
        .iter()
        .map(|signer| resolve_alias_set(aliased_addresses, signer))
        .collect();

    // For each required signer, the distinct signature indices reachable through
    // its alias set. A single zkLogin signature can be reached via two addresses
    // (legacy + modern), so folding through a set collapses those to one index;
    // the sorted order keeps the computed matching deterministic across validators.
    let candidates: Vec<Vec<u8>> = alias_sets
        .iter()
        .map(|alias_set| {
            alias_set
                .iter()
                .filter_map(|alias| sig_mapping.get(alias).map(|(idx, _)| *idx))
                .collect::<BTreeSet<u8>>()
                .into_iter()
                .collect()
        })
        .collect();

    perfect_matching(&candidates)
        .map_err(|signer_idx| signer_signature_absent(&alias_sets[signer_idx], sig_mapping))
}

/// Computes a one-to-one matching of signers to signature indices using Kuhn's
/// algorithm. `candidates[i]` is the set of signature indices signer `i` may be
/// matched to. On success returns the matched index for each signer, in input
/// order; on failure returns the index of the first signer that cannot be matched.
///
/// Kuhn's algorithm augments one signer at a time, so a matching is found whenever
/// one exists. The result is a deterministic function of `candidates`.
fn perfect_matching(candidates: &[Vec<u8>]) -> Result<Vec<u8>, usize> {
    let mut sig_to_signer: BTreeMap<u8, usize> = BTreeMap::new();
    for signer_idx in 0..candidates.len() {
        let mut visited = BTreeSet::new();
        if !find_augmenting_path(signer_idx, candidates, &mut sig_to_signer, &mut visited) {
            return Err(signer_idx);
        }
    }

    let mut signer_to_sig_index: Vec<Option<u8>> = vec![None; candidates.len()];
    for (sig_index, signer_idx) in sig_to_signer {
        signer_to_sig_index[signer_idx] = Some(sig_index);
    }
    Ok(signer_to_sig_index
        .into_iter()
        .map(|sig_index| sig_index.expect("Kuhn's matching binds every signer"))
        .collect())
}

/// Tries to match `signer_idx` to one of its candidate signatures, displacing
/// already-matched signers along an augmenting path where possible. Returns true
/// if `signer_idx` (and everyone it displaced) ends up matched.
fn find_augmenting_path(
    signer_idx: usize,
    candidates: &[Vec<u8>],
    sig_to_signer: &mut BTreeMap<u8, usize>,
    visited: &mut BTreeSet<u8>,
) -> bool {
    for &sig_index in &candidates[signer_idx] {
        if !visited.insert(sig_index) {
            continue;
        }
        let available = match sig_to_signer.get(&sig_index) {
            None => true,
            Some(&current) => find_augmenting_path(current, candidates, sig_to_signer, visited),
        };
        if available {
            sig_to_signer.insert(sig_index, signer_idx);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{Ed25519SuiSignature, Signature};
    use proptest::collection;
    use proptest::prelude::*;

    /// A syntactically well-formed signature. `match_signers_to_signatures` never
    /// inspects the signature itself, so a default one is sufficient for
    /// exercising the matching logic.
    fn dummy_signature() -> GenericSignature {
        Signature::Ed25519SuiSignature(Ed25519SuiSignature::default()).into()
    }

    /// True iff `matching` assigns every signer one of its candidates with no
    /// signature index reused.
    fn is_valid_matching(candidates: &[Vec<u8>], matching: &[u8]) -> bool {
        if matching.len() != candidates.len() {
            return false;
        }
        let mut used = BTreeSet::new();
        matching
            .iter()
            .enumerate()
            .all(|(signer, sig)| candidates[signer].contains(sig) && used.insert(*sig))
    }

    /// Brute-force reference: does any perfect matching exist?
    fn brute_force_has_matching(candidates: &[Vec<u8>]) -> bool {
        fn recurse(candidates: &[Vec<u8>], signer: usize, used: &mut BTreeSet<u8>) -> bool {
            if signer == candidates.len() {
                return true;
            }
            for &sig in &candidates[signer] {
                if used.insert(sig) {
                    if recurse(candidates, signer + 1, used) {
                        return true;
                    }
                    used.remove(&sig);
                }
            }
            false
        }
        recurse(candidates, 0, &mut BTreeSet::new())
    }

    #[test]
    fn perfect_matching_basic_cases() {
        // No signers: trivially matched.
        assert_eq!(perfect_matching(&[]), Ok(vec![]));
        // Single signer, single candidate (the index need not be positional).
        assert_eq!(perfect_matching(&[vec![2]]), Ok(vec![2]));
        // A signer with no candidate cannot be matched.
        assert_eq!(perfect_matching(&[vec![]]), Err(0));
        // Disjoint candidates.
        assert_eq!(perfect_matching(&[vec![0], vec![1]]), Ok(vec![0, 1]));
        // Two signers competing for the same single signature.
        assert_eq!(perfect_matching(&[vec![0], vec![0]]), Err(1));
    }

    #[test]
    fn perfect_matching_finds_augmenting_paths() {
        // A greedy assignment would give signer 0 -> 0 and strand signer 1;
        // Kuhn's must displace signer 0 onto index 1.
        let candidates = vec![vec![0], vec![0, 1]];
        let matching = perfect_matching(&candidates).unwrap();
        assert!(is_valid_matching(&candidates, &matching));

        // A longer displacement chain across four signers.
        let candidates = vec![vec![0], vec![0, 1], vec![1, 2], vec![2, 3]];
        let matching = perfect_matching(&candidates).unwrap();
        assert!(is_valid_matching(&candidates, &matching));
    }

    #[test]
    fn perfect_matching_rejects_when_no_matching_exists() {
        // Every signer has a candidate, but the candidates cannot all be
        // satisfied simultaneously.
        assert!(perfect_matching(&[vec![0], vec![0], vec![1]]).is_err());
        assert!(perfect_matching(&[vec![0, 1], vec![0, 1], vec![0, 1]]).is_err());
        // ...whereas a feasible three-signer instance succeeds.
        let candidates = vec![vec![0], vec![0, 1], vec![1, 2]];
        assert!(is_valid_matching(
            &candidates,
            &perfect_matching(&candidates).unwrap(),
        ));
    }

    #[test]
    fn perfect_matching_handles_any_within_signer_order() {
        // The production code always feeds sorted candidate lists, but the
        // algorithm must still find a valid matching for any ordering.
        for candidates in [vec![vec![0, 1], vec![0]], vec![vec![1, 0], vec![0]]] {
            let matching = perfect_matching(&candidates).unwrap();
            assert!(is_valid_matching(&candidates, &matching));
        }
    }

    proptest! {
        // `perfect_matching` agrees with a brute-force reference: it returns Ok
        // exactly when a perfect matching exists, and any matching it returns is
        // valid. The candidate lists are sorted/deduplicated to mirror the output
        // of `signer_candidates`.
        #[test]
        fn perfect_matching_agrees_with_brute_force(
            candidates in collection::vec(collection::vec(0u8..6, 0..6), 0..6)
        ) {
            let candidates: Vec<Vec<u8>> = candidates
                .into_iter()
                .map(|mut c| {
                    c.sort_unstable();
                    c.dedup();
                    c
                })
                .collect();

            let result = perfect_matching(&candidates);
            prop_assert_eq!(result.is_ok(), brute_force_has_matching(&candidates));
            if let Ok(matching) = result {
                prop_assert!(is_valid_matching(&candidates, &matching));
            }
        }
    }

    #[test]
    fn match_signers_to_signatures_binds_each_signer_to_a_distinct_signature() {
        let dummy = dummy_signature();
        let sender = SuiAddress::random_for_testing_only();
        let sponsor = SuiAddress::random_for_testing_only();
        let sender_alias = SuiAddress::random_for_testing_only();
        let sponsor_alias = SuiAddress::random_for_testing_only();

        let sig_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> = [
            (sender_alias, (0u8, &dummy)),
            (sponsor_alias, (1u8, &dummy)),
        ]
        .into_iter()
        .collect();
        let aliased = vec![
            (sender, NonEmpty::new(sender_alias)),
            (sponsor, NonEmpty::new(sponsor_alias)),
        ];
        let mut required = NonEmpty::new(sender);
        required.push(sponsor);

        assert_eq!(
            match_signers_to_signatures(&required, &aliased, &sig_mapping).unwrap(),
            vec![0, 1],
        );

        // A required signer with no reachable signature is reported absent.
        let lonely_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(sender_alias, (0u8, &dummy))].into_iter().collect();
        assert!(matches!(
            match_signers_to_signatures(&required, &aliased, &lonely_mapping)
                .unwrap_err()
                .into_inner(),
            SuiErrorKind::SignerSignatureAbsent { .. },
        ));

        // Two required signers whose only reachable signature is the same one
        // cannot both be bound: no one-to-one matching exists.
        let shared_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(sender_alias, (0u8, &dummy))].into_iter().collect();
        let both_alias_sender = vec![
            (sender, NonEmpty::new(sender_alias)),
            (sponsor, NonEmpty::new(sender_alias)),
        ];
        assert!(matches!(
            match_signers_to_signatures(&required, &both_alias_sender, &shared_mapping)
                .unwrap_err()
                .into_inner(),
            SuiErrorKind::SignerSignatureAbsent { .. },
        ));
    }

    #[test]
    fn greedy_match_signers_to_signatures_assigns_first_reachable_signature() {
        let dummy = dummy_signature();
        let sender = SuiAddress::random_for_testing_only();
        let sponsor = SuiAddress::random_for_testing_only();
        let alias_a = SuiAddress::random_for_testing_only();
        let alias_b = SuiAddress::random_for_testing_only();

        let mut required = NonEmpty::new(sender);
        required.push(sponsor);

        // Happy path: no aliases, each required signer has its own signature.
        let sig_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(sender, (0u8, &dummy)), (sponsor, (1u8, &dummy))]
                .into_iter()
                .collect();
        assert_eq!(
            greedy_match_signers_to_signatures(&required, &[], &sig_mapping).unwrap(),
            vec![0, 1],
        );

        // Within an alias set, the first reachable signature wins (alias-set
        // order), even when a later alias would also match.
        let mut sender_aliases = NonEmpty::new(alias_a);
        sender_aliases.push(alias_b);
        let aliased = vec![(sender, sender_aliases)];
        let sig_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(alias_a, (5u8, &dummy)), (alias_b, (3u8, &dummy))]
                .into_iter()
                .collect();
        assert_eq!(
            greedy_match_signers_to_signatures(&NonEmpty::new(sender), &aliased, &sig_mapping)
                .unwrap(),
            vec![5],
        );

        // The legacy path does not enforce a one-to-one matching: two required
        // signers whose alias sets both resolve to the same address are both
        // bound to that one signature. `match_signers_to_signatures` rejects the
        // exact same input.
        let aliased = vec![(sponsor, NonEmpty::new(sender))];
        let sig_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(sender, (0u8, &dummy)), (alias_a, (1u8, &dummy))]
                .into_iter()
                .collect();
        assert_eq!(
            greedy_match_signers_to_signatures(&required, &aliased, &sig_mapping).unwrap(),
            vec![0, 0],
        );
        assert!(matches!(
            match_signers_to_signatures(&required, &aliased, &sig_mapping)
                .unwrap_err()
                .into_inner(),
            SuiErrorKind::SignerSignatureAbsent { .. },
        ));

        // A required signer with no reachable signature is reported absent.
        let sig_mapping: BTreeMap<SuiAddress, (u8, &GenericSignature)> =
            [(sender, (0u8, &dummy))].into_iter().collect();
        assert!(matches!(
            greedy_match_signers_to_signatures(&required, &[], &sig_mapping)
                .unwrap_err()
                .into_inner(),
            SuiErrorKind::SignerSignatureAbsent { .. },
        ));
    }
}
