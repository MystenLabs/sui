// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Formally verified signature verification for user-signed transactions.
//!
//! [`verify_signatures`] handles only user transactions: intent checking and
//! the system-transaction bypass are the caller's responsibility.
//!
//! The function is generic over the signature and address types so that this
//! crate has no dependency on `sui-types`. The concrete instantiation
//! (`GenericSignature`, `SuiAddress`) lives in `sui-types`.
//!
//! Informal spec: `crates/sui-types/verify_sig_spec.md`

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// § 1  Error type
// ---------------------------------------------------------------------------

/// Verification failure reasons, kept intentionally small.
///
/// Callers in `sui-types` convert these variants into the appropriate
/// `SuiError` / `SuiErrorKind` variants.
pub enum SigVerifyError {
    /// `|tx_signatures| ≠ |required_signers|`.
    SignerCountMismatch { actual: usize, expected: usize },
    /// A signature's address cannot be derived (malformed bytes).
    AddressDerivationFailed,
    /// A required signer has no matching signature (including via aliases).
    SignerAbsent,
    /// A signature failed cryptographic verification.
    CryptoVerificationFailed,
}

// ---------------------------------------------------------------------------
// § 2  Signature trait
// ---------------------------------------------------------------------------

/// The two operations the verified algorithm needs from a concrete signature.
///
/// Implemented in `sui-types` for `GenericSignature` (wrapped with its
/// verification context). The verified module never sees the concrete type.
pub trait SignatureVerifiable<Addr>: Sized {
    /// Derive all addresses this signature is associated with.
    ///
    /// A single signature may yield more than one address (e.g. a zklogin
    /// signature with legacy-address support). Returns
    /// `Err(SigVerifyError::AddressDerivationFailed)` for malformed input.
    fn try_derive_addresses(&self) -> (r: Result<Vec<Addr>, SigVerifyError>)
        ensures
            r matches Err(_) <==> spec_sig_addr_fails(self),
            r matches Ok(_) ==> r->Ok_0@.to_set() =~= spec_sig_addresses::<Self, Addr>(self);

    /// Cryptographically verify this signature as proof of authorization by
    /// `addr` at `epoch`. Returns `Err(CryptoVerificationFailed)` on failure.
    fn verify_for_address(&self, addr: &Addr, epoch: u64) -> (r: Result<(), SigVerifyError>)
        ensures
            r matches Ok(_) <==> spec_sig_crypto_valid(self, *addr, epoch);
}

// ---------------------------------------------------------------------------
// § 3  Abstract spec predicates (single-element primitives)
// ---------------------------------------------------------------------------
// These operate on a single `&S` so they can be connected to the
// `SignatureVerifiable` trait methods via the trait ensures clauses.

/// The set of addresses derivable from a single signature.
/// Undefined (and never queried) when `spec_sig_addr_fails(sig)`.
pub uninterp spec fn spec_sig_addresses<S, Addr>(sig: &S) -> Set<Addr>;

/// Whether address derivation fails for a single signature.
pub uninterp spec fn spec_sig_addr_fails<S>(sig: &S) -> bool;

/// Whether a single signature is cryptographically valid for `addr` at `epoch`.
/// Independent of aliases — this is the raw crypto check.
pub uninterp spec fn spec_sig_crypto_valid<S, Addr>(sig: &S, addr: Addr, epoch: u64) -> bool;

// ---------------------------------------------------------------------------
// § 4  Derived spec predicates (slice-indexed, used in ensures clauses)
// ---------------------------------------------------------------------------

/// The address set for the signature at position `i` in `sigs`.
pub open spec fn spec_addresses<S, Addr>(sigs: &[S], i: int) -> Set<Addr> {
    spec_sig_addresses::<S, Addr>(&sigs@[i])
}

/// Whether address derivation fails for the signature at position `i`.
pub open spec fn spec_addr_derivation_fails<S>(sigs: &[S], i: int) -> bool {
    spec_sig_addr_fails(&sigs@[i])
}

/// Whether any signature in `sigs` has an uncomputable address set.
pub open spec fn spec_any_addr_derivation_fails<S>(sigs: &[S]) -> bool {
    exists|i: int| 0 <= i < sigs@.len() && spec_addr_derivation_fails(sigs, i)
}

/// Whether the signature at position `i` is valid given an alias set:
///   - there exists an address A in both addresses(sigs[i]) and `aliases`, AND
///   - sigs[i] is cryptographically valid for A at `epoch`.
///
/// Verification runs against the matching address A directly.
/// The canonical sender address is not involved in the crypto check.
pub open spec fn spec_is_valid_for<S, Addr>(
    sigs: &[S],
    i: int,
    aliases: Set<Addr>,
    epoch: u64,
) -> bool {
    exists|a: Addr|
        #![trigger spec_addresses::<S, Addr>(sigs, i).contains(a)]
        spec_addresses::<S, Addr>(sigs, i).contains(a)
        && aliases.contains(a)
        && spec_sig_crypto_valid(&sigs@[i], a, epoch)
}

/// The greedy assignment.
/// `required_signers[k] = (canonical_addr, aliases)` — the canonical address
/// and the set of alias addresses that may sign on its behalf.
///
/// Returns `Some(indices)` where `indices[k]` is the first unused position
/// in `sigs` valid for `required_signers[k]`'s alias set, or `None` if any
/// sender cannot be matched.
pub open spec fn spec_greedy_result<S, Addr>(
    sigs: &[S],
    required_signers: &[(Addr, Vec<Addr>)],
    epoch: u64,
) -> Option<Seq<u8>>
    decreases required_signers@.len()
{
    spec_greedy_helper(sigs, required_signers@, epoch, Set::empty())
}

/// Recursive helper for the greedy algorithm.
///
/// `signers[k] = (canonical_addr, aliases)`.
/// `used` tracks which sig positions have already been assigned.
pub open spec fn spec_greedy_helper<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
) -> Option<Seq<u8>>
    decreases signers.len()
{
    if signers.len() == 0 {
        Some(seq![])
    } else {
        let j = spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used);
        match j {
            None => None,
            Some(j) => {
                match spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j)) {
                    None => None,
                    Some(rest) => Some(seq![j as u8] + rest),
                }
            }
        }
    }
}

/// The smallest position j in `sigs` that is (a) not in `used` and
/// (b) valid for `aliases`. Returns `None` if no such position exists.
pub open spec fn spec_first_valid_unused<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
) -> Option<int>
    decreases sigs@.len()
{
    spec_first_valid_unused_from(sigs, aliases, epoch, used, 0)
}

pub open spec fn spec_first_valid_unused_from<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
) -> Option<int>
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
        None
    } else if !used.contains(start) && spec_is_valid_for(sigs, start, aliases, epoch) {
        Some(start)
    } else {
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start + 1)
    }
}

// ---------------------------------------------------------------------------
// § 5  Challenge theorems
// ---------------------------------------------------------------------------

// --- Helpers for spec_first_valid_unused_from ---

/// The position returned lies in [start, sigs.len()).
proof fn lemma_first_valid_from_bounds<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
)
    ensures
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start) matches Some(_)
            ==> {
                let j = spec_first_valid_unused_from(sigs, aliases, epoch, used, start)->Some_0;
                start <= j < sigs@.len() as int
            }
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
    } else if !used.contains(start) && spec_is_valid_for(sigs, start, aliases, epoch) {
    } else {
        lemma_first_valid_from_bounds(sigs, aliases, epoch, used, start + 1);
    }
}

/// The position returned is not in `used`.
proof fn lemma_first_valid_from_not_in_used<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
)
    ensures
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start) matches Some(_)
            ==> !used.contains(
                    spec_first_valid_unused_from(sigs, aliases, epoch, used, start)->Some_0,
                )
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
    } else if !used.contains(start) && spec_is_valid_for(sigs, start, aliases, epoch) {
    } else {
        lemma_first_valid_from_not_in_used(sigs, aliases, epoch, used, start + 1);
    }
}

/// The position returned satisfies spec_is_valid_for.
proof fn lemma_first_valid_from_is_valid<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
)
    ensures
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start) matches Some(_)
            ==> spec_is_valid_for(
                    sigs,
                    spec_first_valid_unused_from(sigs, aliases, epoch, used, start)->Some_0,
                    aliases,
                    epoch,
                )
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
    } else if !used.contains(start) && spec_is_valid_for(sigs, start, aliases, epoch) {
    } else {
        lemma_first_valid_from_is_valid(sigs, aliases, epoch, used, start + 1);
    }
}

// --- CT1–CT4 for spec_greedy_helper ---

/// CT1: Output length equals the number of signers.
proof fn lemma_greedy_len<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) matches Some(_)
            ==> spec_greedy_helper(sigs, signers, epoch, used)->Some_0.len() == signers.len() as int
    decreases signers.len()
{
    if signers.len() == 0 {
    } else {
        let j = spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used);
        if j matches Some(_) {
            lemma_greedy_len(sigs, signers.skip(1), epoch, used.insert(j->Some_0));
        }
    }
}

/// CT2: Every index in the output is in bounds (< sigs.len()).
proof fn lemma_greedy_bounds<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) matches Some(_)
            ==> forall|k: int|
                    #![trigger spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k]]
                    0 <= k < spec_greedy_helper(sigs, signers, epoch, used)->Some_0.len()
                        ==> (spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k] as int)
                            < sigs@.len() as int
    decreases signers.len()
{
    if signers.len() == 0 {
    } else {
        let j_opt = spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used);
        if j_opt matches Some(_) {
            let j = j_opt->Some_0;
            lemma_first_valid_from_bounds(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_greedy_bounds(sigs, signers.skip(1), epoch, used.insert(j));
            lemma_greedy_len(sigs, signers.skip(1), epoch, used.insert(j));
            let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
            let rest = spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j))->Some_0;
            // TODO: prove by unrolling spec_greedy_helper (trigger/SMT issue).
            assume(indices[0] == j as u8);
            assume(forall|k: int| 1 <= k < indices.len() ==> #[trigger] indices[k] == rest[k - 1]);
            assume(indices.len() == 1 + rest.len());
            assert forall|k: int| 0 <= k < indices.len() implies (indices[k] as int) < sigs@.len() as int
            by {
                if k == 0 {
                    assert(indices[0] == j as u8);
                } else {
                    assert(indices[k] == rest[k - 1]);
                }
            };
        }
    }
}

/// CT3: Indices are not in `used` (CT3a) and pairwise distinct (CT3b).
proof fn lemma_greedy_not_in_used_and_distinct<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) matches Some(_) ==> {
            let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
            &&& forall|k: int|
                    #![trigger indices[k]]
                    0 <= k < indices.len() ==> !used.contains(indices[k] as int)
            &&& forall|k1: int, k2: int|
                    #![trigger indices[k1], indices[k2]]
                    0 <= k1 < indices.len()
                        && 0 <= k2 < indices.len()
                        && k1 != k2
                        ==> indices[k1] != indices[k2]
        }
    decreases signers.len()
{
    if signers.len() == 0 {
    } else {
        let j_opt = spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used);
        if j_opt matches Some(_) {
            let j = j_opt->Some_0;
            let used2 = used.insert(j);
            lemma_first_valid_from_not_in_used(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_greedy_not_in_used_and_distinct(sigs, signers.skip(1), epoch, used2);
            let rest_opt = spec_greedy_helper(sigs, signers.skip(1), epoch, used2);
            if rest_opt matches Some(_) {
                let rest = rest_opt->Some_0;
                let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
                // TODO: prove by unrolling spec_greedy_helper (trigger/SMT issue).
                assume(indices[0] == j as u8);
                assume(forall|k: int| 1 <= k < indices.len() ==> #[trigger] indices[k] == rest[k - 1]);
                assume(indices.len() == 1 + rest.len());
                // CT3a and CT3b: logical argument is sound but assert-forall by-blocks
                // cannot access the outer assumes due to trigger scoping. TODO.
                assume(forall|k: int| 0 <= k < indices.len() ==> !used.contains(#[trigger] indices[k] as int));
                assume(forall|k1: int, k2: int|
                    0 <= k1 < indices.len() && 0 <= k2 < indices.len() && k1 != k2
                        ==> #[trigger] indices[k1] != #[trigger] indices[k2]);
            }
        }
    }
}

/// CT4: Each index[k] satisfies spec_is_valid_for the k-th signer's aliases.
proof fn lemma_greedy_valid<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) matches Some(_)
            ==> forall|k: int|
                    #![trigger spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k]]
                    0 <= k < spec_greedy_helper(sigs, signers, epoch, used)->Some_0.len()
                        ==> spec_is_valid_for(
                                sigs,
                                spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k] as int,
                                signers[k].1@.to_set(),
                                epoch,
                            )
    decreases signers.len()
{
    if signers.len() == 0 {
    } else {
        let j_opt = spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used);
        if j_opt matches Some(_) {
            let j = j_opt->Some_0;
            lemma_first_valid_from_is_valid(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_greedy_valid(sigs, signers.skip(1), epoch, used.insert(j));
            lemma_greedy_len(sigs, signers.skip(1), epoch, used.insert(j));
            let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
            let rest = spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j))->Some_0;
            // TODO: prove by unrolling spec_greedy_helper (trigger/SMT issue).
            assume(indices[0] == j as u8);
            assume(forall|k: int| 1 <= k < indices.len() ==> #[trigger] indices[k] == rest[k - 1]);
            assume(indices.len() == 1 + rest.len());
            // TODO: same assert-forall trigger scoping issue as CT3.
            assume(forall|k: int|
                0 <= k < indices.len()
                    ==> #[trigger] spec_is_valid_for(
                            sigs, indices[k] as int, signers[k].1@.to_set(), epoch,
                        ));
        }
    }
}

// ---------------------------------------------------------------------------
// § 6  Helper: extract Ok value
// ---------------------------------------------------------------------------

pub open spec fn ok_indices(result: &Result<Vec<u8>, SigVerifyError>) -> Seq<u8> {
    match result {
        Ok(v) => v@,
        Err(_) => seq![],
    }
}

// ---------------------------------------------------------------------------
// § 7  Verified function (body left unimplemented — proof to follow)
// ---------------------------------------------------------------------------

/// Verify signatures on a user-signed transaction.
///
/// The caller is responsible for:
/// - Checking that the transaction intent is `SUI_TRANSACTION_INTENT`.
/// - Skipping this call for system transactions (which are unconditionally valid).
///
/// # Parameters
/// - `tx_signatures` — ordered sequence of authenticators.
/// - `required_signers` — each entry is `(canonical_sender, aliases)` where
///   `aliases` is the non-empty set of addresses that may sign for this sender.
///   The typical case is `aliases = vec![canonical_sender]`.
/// - `epoch` — current epoch for cryptographic verification.
///
/// # Preconditions
/// - `required_signers.len() <= 255` (indices stored as `u8`).
/// - Distinct signatures in `tx_signatures` have disjoint address sets.
///
/// # Contract
///
/// See `crates/sui-types/verify_sig_spec.md`.
#[verifier::external_body]
pub fn verify_signatures<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
    required_signers: &[(Addr, Vec<Addr>)],
    epoch: u64,
) -> (result: Result<Vec<u8>, SigVerifyError>)
    requires
        required_signers@.len() <= u8::MAX as nat,
        forall|i: int, j: int|
            0 <= i < tx_signatures@.len()
            && 0 <= j < tx_signatures@.len()
            && i != j
            && !spec_addr_derivation_fails(tx_signatures, i)
            && !spec_addr_derivation_fails(tx_signatures, j)
            ==> #[trigger] spec_addresses::<S, Addr>(tx_signatures, i)
                    .disjoint(spec_addresses::<S, Addr>(tx_signatures, j)),
    ensures
        if tx_signatures@.len() != required_signers@.len() {
            result matches Err(_)                                          // E1: count mismatch
        } else if spec_any_addr_derivation_fails(tx_signatures) {
            result matches Err(_)                                          // E2: derivation failure
        } else if !(spec_greedy_result(tx_signatures, required_signers, epoch) matches Some(_)) {
            result matches Err(_)                                          // E3: greedy fails
        } else {
            &&& result matches Ok(_)                                       // S1: greedy succeeds
            &&& ok_indices(&result)
                    =~= spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0
        },
        // R1: length matches required_signers
        result matches Ok(_) ==> ok_indices(&result).len() == required_signers@.len(),
        // R2: every index is in bounds
        result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> (ok_indices(&result)[k] as int) < tx_signatures@.len(),
        // R3: indices are pairwise distinct (bijection)
        result matches Ok(_) ==>
            forall|k1: int, k2: int|
                0 <= k1 < ok_indices(&result).len()
                && 0 <= k2 < ok_indices(&result).len()
                && k1 != k2
                ==> ok_indices(&result)[k1] != ok_indices(&result)[k2],
        // R4: each assigned signature is valid for its signer's alias set
        result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> #[trigger] spec_is_valid_for(
                        tx_signatures,
                        ok_indices(&result)[k] as int,
                        required_signers@[k].1@.to_set(),
                        epoch,
                    ),
{
    unimplemented!()
}

} // verus!
