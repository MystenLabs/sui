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

#[cfg(verus_only)]
use crate::utils::nonempty_view;
use crate::utils::{clone_and_set, prepend_u8, slice_contains};
use nonempty::NonEmpty;
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

/// Whether a single signature is valid given an alias set:
///   - there exists an address A in both addresses(sig) and `aliases`, AND
///   - sig is cryptographically valid for A at `epoch`.
///
/// Verification runs against the matching address A directly.
/// The canonical sender address is not involved in the crypto check.
pub open spec fn spec_is_valid_for<S, Addr>(
    sig: &S,
    aliases: Set<Addr>,
    epoch: u64,
) -> bool {
    exists|a: Addr|
        #![trigger spec_sig_addresses::<S, Addr>(sig).contains(a)]
        spec_sig_addresses::<S, Addr>(sig).contains(a)
        && aliases.contains(a)
        && spec_sig_crypto_valid(sig, a, epoch)
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
    } else if !used.contains(start) && spec_is_valid_for(&sigs@[start], aliases, epoch) {
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
    } else if !used.contains(start) && spec_is_valid_for(&sigs@[start], aliases, epoch) {
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
    } else if !used.contains(start) && spec_is_valid_for(&sigs@[start], aliases, epoch) {
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
                    &sigs@[spec_first_valid_unused_from(sigs, aliases, epoch, used, start)->Some_0],
                    aliases,
                    epoch,
                )
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
    } else if !used.contains(start) && spec_is_valid_for(&sigs@[start], aliases, epoch) {
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
            let rec_opt = spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j));
            if rec_opt matches Some(_) {
                let rest = rec_opt->Some_0;
                let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
                lemma_greedy_helper_unfold::<S, Addr>(sigs, signers, epoch, used, j, rest);
                assert(indices.len() == 1 + rest.len());
                assert(indices[0] == j as u8);
                assert forall|k: int| 1 <= k < indices.len() implies indices[k] == rest[k - 1] by {
                    assert(indices[k] == (seq![j as u8] + rest)[k]);
                };
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
}

/// CT3: Indices are not in `used` (CT3a) and pairwise distinct (CT3b).
proof fn lemma_greedy_not_in_used_and_distinct<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    requires
        sigs@.len() <= u8::MAX as nat,
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
            lemma_first_valid_from_bounds(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_first_valid_from_not_in_used(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_greedy_not_in_used_and_distinct(sigs, signers.skip(1), epoch, used2);
            let rest_opt = spec_greedy_helper(sigs, signers.skip(1), epoch, used2);
            if rest_opt matches Some(_) {
                let rest = rest_opt->Some_0;
                let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
                lemma_greedy_helper_unfold::<S, Addr>(sigs, signers, epoch, used, j, rest);
                assert(indices.len() == 1 + rest.len());
                assert(indices[0] == j as u8);
                assert forall|k: int| 1 <= k < indices.len() implies indices[k] == rest[k - 1] by {
                    assert(indices[k] == (seq![j as u8] + rest)[k]);
                };
                // j is not in used (from lemma_first_valid_from_not_in_used applied above)
                assert(!used.contains(j));
                // Abbreviate spec_greedy_helper(sigs, signers.skip(1), epoch, used2)->Some_0 as rest2
                // to correctly trigger the IH's forall quantifier
                let rest2 = spec_greedy_helper(sigs, signers.skip(1), epoch, used2)->Some_0;
                assert(rest2 =~= rest);
                // CT3a: indices[k] not in used
                assert forall|k: int| 0 <= k < indices.len() implies !used.contains(#[trigger] indices[k] as int)
                by {
                    if k == 0 {
                        assert(indices[0] == j as u8);
                        assert(!used.contains(j));
                        // j in [0, sigs.len()) ⊆ [0, u8::MAX], so j as u8 as int = j
                        // From lemma_first_valid_from_bounds with start=0: 0 <= j < sigs.len()
                        assert(0 <= j < sigs@.len() as int);
                        assert(sigs@.len() as nat <= u8::MAX as nat);
                        assert(indices[0] as int == j);
                    } else {
                        assert(indices[k] == rest[k - 1]);
                        // IH trigger: !used2.contains(rest2[k-1] as int)
                        assert(!used2.contains(rest2[k - 1] as int));
                        assert(rest2[k - 1] == rest[k - 1]);
                        // used ⊆ used2: !used2.contains(x) ==> !used.contains(x)
                        // Because used2 = used.insert(j), so used.contains(x) => used2.contains(x)
                        // Contrapositive: !used2.contains(x) => !used.contains(x)
                        assert(!used.contains(rest[k - 1] as int)) by {
                            assert(used2 =~= used.insert(j));
                            if used.contains(rest[k - 1] as int) {
                                // then used2.contains(rest[k-1] as int) since used ⊆ used2
                                assert(used2.contains(rest[k - 1] as int));
                            }
                        };
                        assert(indices[k] as int == rest[k - 1] as int);
                    }
                };
                // CT3b: indices are pairwise distinct
                assert forall|k1: int, k2: int|
                    0 <= k1 < indices.len() && 0 <= k2 < indices.len() && k1 != k2
                        implies #[trigger] indices[k1] != #[trigger] indices[k2]
                by {
                    if k1 == 0 && k2 > 0 {
                        assert(indices[0] == j as u8);
                        assert(indices[k2] == rest[k2 - 1]);
                        assert(!used2.contains(rest2[k2 - 1] as int));
                        assert(rest2[k2 - 1] == rest[k2 - 1]);
                        assert(used2.contains(j));
                        // !used2.contains(rest[k2-1] as int) but used2.contains(j) → rest[k2-1] as int != j
                        assert(rest[k2 - 1] as int != j);
                        // j as u8 == indices[0] and rest[k2-1] == indices[k2]
                        // so indices[0] as int = j != rest[k2-1] as int = indices[k2] as int
                        assert(indices[0] as int != indices[k2] as int);
                    } else if k1 > 0 && k2 == 0 {
                        assert(indices[0] == j as u8);
                        assert(indices[k1] == rest[k1 - 1]);
                        assert(!used2.contains(rest2[k1 - 1] as int));
                        assert(rest2[k1 - 1] == rest[k1 - 1]);
                        assert(used2.contains(j));
                        assert(rest[k1 - 1] as int != j);
                        assert(indices[0] as int != indices[k1] as int);
                    } else {
                        // Both k1, k2 > 0: indices[k1] = rest[k1-1], indices[k2] = rest[k2-1]
                        // Distinct by IH on rest2
                        assert(indices[k1] == rest[k1 - 1]);
                        assert(indices[k2] == rest[k2 - 1]);
                        assert(rest2[k1 - 1] == rest[k1 - 1]);
                        assert(rest2[k2 - 1] == rest[k2 - 1]);
                        // IH gives: rest2[k1-1] != rest2[k2-1] (pairwise distinct for signers.skip(1))
                        assert(rest2[k1 - 1] != rest2[k2 - 1]);
                        assert(indices[k1] != indices[k2]);
                    }
                };
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
    requires
        sigs@.len() <= u8::MAX as nat,
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) matches Some(_)
            ==> forall|k: int|
                    #![trigger spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k]]
                    0 <= k < spec_greedy_helper(sigs, signers, epoch, used)->Some_0.len()
                        ==> spec_is_valid_for(
                                &sigs@[
                                    spec_greedy_helper(sigs, signers, epoch, used)->Some_0[k] as int
                                ],
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
            lemma_first_valid_from_bounds(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_first_valid_from_is_valid(sigs, signers[0].1@.to_set(), epoch, used, 0);
            lemma_greedy_valid(sigs, signers.skip(1), epoch, used.insert(j));
            lemma_greedy_len(sigs, signers.skip(1), epoch, used.insert(j));
            let rec_opt = spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j));
            if rec_opt matches Some(_) {
                let rest = rec_opt->Some_0;
                let indices = spec_greedy_helper(sigs, signers, epoch, used)->Some_0;
                lemma_greedy_helper_unfold::<S, Addr>(sigs, signers, epoch, used, j, rest);
                assert(indices.len() == 1 + rest.len());
                assert(indices[0] == j as u8);
                assert forall|k: int| 1 <= k < indices.len() implies indices[k] == rest[k - 1] by {
                    assert(indices[k] == (seq![j as u8] + rest)[k]);
                };
                // CT4: spec_is_valid_for for each index
                assert forall|k: int|
                    0 <= k < indices.len()
                        implies #[trigger] spec_is_valid_for(
                                &sigs@[indices[k] as int], signers[k].1@.to_set(), epoch,
                            )
                by {
                    if k == 0 {
                        assert(indices[0] == j as u8);
                        // j in [0, sigs.len()) ⊆ [0, u8::MAX], so j as u8 as int = j
                        assert(0 <= j < sigs@.len() as int);
                        assert(sigs@.len() as nat <= u8::MAX as nat);
                        assert(indices[0] as int == j);
                        assert(spec_is_valid_for(&sigs@[j], signers[0].1@.to_set(), epoch));
                    } else {
                        assert(indices[k] == rest[k - 1]);
                        assert(signers.skip(1)[k - 1] == signers[k]);
                        // Trigger the IH by naming the helper result at position k-1
                        let rest_k = spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j))->Some_0[k - 1];
                        assert(rest_k == rest[k - 1]);
                        assert(spec_is_valid_for(&sigs@[rest_k as int], signers.skip(1)[k - 1].1@.to_set(), epoch));
                        assert(signers.skip(1)[k - 1].1@.to_set() =~= signers[k].1@.to_set());
                        assert(indices[k] as int == rest_k as int);
                    }
                };
            }
        }
    }
}

/// spec_greedy_helper with known first_valid_unused=Some(j) and recursive=Some(rest)
/// returns Some(seq![j as u8] + rest). Body empty: follows from the open spec fn definition.
proof fn lemma_greedy_helper_unfold<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
    j: int,
    rest: Seq<u8>,
)
    requires
        signers.len() > 0,
        spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used) == Some(j),
        spec_greedy_helper(sigs, signers.skip(1), epoch, used.insert(j)) == Some(rest),
    ensures
        spec_greedy_helper(sigs, signers, epoch, used) == Some(seq![j as u8] + rest)
{
    // Follows directly from unrolling spec_greedy_helper: signers.len() > 0, j = Some(j), rest = Some(rest)
}

/// spec_greedy_helper with first_valid_unused=None returns None.
proof fn lemma_greedy_helper_none_first<S, Addr>(
    sigs: &[S],
    signers: Seq<(Addr, Vec<Addr>)>,
    epoch: u64,
    used: Set<int>,
)
    requires
        signers.len() > 0,
        !(spec_first_valid_unused(sigs, signers[0].1@.to_set(), epoch, used) matches Some(_)),
    ensures
        !(spec_greedy_helper(sigs, signers, epoch, used) matches Some(_))
{
    // Follows directly from unrolling spec_greedy_helper: signers.len() > 0, j = None → None
}

/// If all unused positions in [start, j) satisfy: used OR !spec_is_valid_for,
/// and position j is unused and valid, then spec_first_valid_unused_from(start) = Some(j).
proof fn lemma_first_valid_from_correct<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
    j: int,
)
    requires
        start <= j < sigs@.len() as int,
        !used.contains(j),
        spec_is_valid_for(&sigs@[j], aliases, epoch),
        forall|k: int| start <= k < j ==>
            used.contains(k) || !spec_is_valid_for(&sigs@[k], aliases, epoch),
    ensures
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start) == Some(j)
    decreases j - start
{
    if start == j {
        // !used.contains(j) && spec_is_valid_for → returns Some(j) by definition
    } else {
        assert(used.contains(start) || !spec_is_valid_for(&sigs@[start], aliases, epoch));
        lemma_first_valid_from_correct(sigs, aliases, epoch, used, start + 1, j);
    }
}

// ---------------------------------------------------------------------------
// § 6  Slice-contains helper
// ---------------------------------------------------------------------------

/// Check whether a slice contains an element.  Proven directly from the loop
// ---------------------------------------------------------------------------
// § 7  Lemma: first_valid_from_none for the invalid (not just disjoint) case
// ---------------------------------------------------------------------------

/// If every unused position k in [start, sigs.len()) has !spec_is_valid_for,
/// then spec_first_valid_unused_from(start) = None.
///
/// Strictly stronger than `lemma_first_valid_from_none` (which requires
/// disjoint address sets); this only requires !spec_is_valid_for.
proof fn lemma_first_valid_from_none_invalid<S, Addr>(
    sigs: &[S],
    aliases: Set<Addr>,
    epoch: u64,
    used: Set<int>,
    start: int,
)
    requires
        forall|k: int|
            start <= k < sigs@.len() as int && !used.contains(k)
            ==> !spec_is_valid_for(&#[trigger] sigs@[k], aliases, epoch),
    ensures
        spec_first_valid_unused_from(sigs, aliases, epoch, used, start) matches None
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
    } else if used.contains(start) {
        lemma_first_valid_from_none_invalid(sigs, aliases, epoch, used, start + 1);
    } else {
        assert(!spec_is_valid_for(&sigs@[start], aliases, epoch));
        lemma_first_valid_from_none_invalid(sigs, aliases, epoch, used, start + 1);
    }
}

// ---------------------------------------------------------------------------
// § 8  Helper: extract Ok value
// ---------------------------------------------------------------------------

pub open spec fn ok_indices(result: &Result<Vec<u8>, SigVerifyError>) -> Seq<u8> {
    match result {
        Ok(v) => v@,
        Err(_) => seq![],
    }
}

// ---------------------------------------------------------------------------
// § 7  Address derivation helper
// ---------------------------------------------------------------------------

/// Derive addresses for every signature in one pass.
/// Returns `Err` if any signature is malformed, otherwise `Ok(sig_addrs)`
/// where `sig_addrs[i]@.to_set() == spec_addresses(tx_signatures, i)`.
fn derive_all_addresses<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
) -> (result: Result<Vec<Vec<Addr>>, SigVerifyError>)
    ensures
        result matches Err(_) ==> spec_any_addr_derivation_fails(tx_signatures),
        result matches Ok(_) ==> !spec_any_addr_derivation_fails(tx_signatures),
        result matches Ok(sig_addrs) ==> {
            &&& sig_addrs@.len() == tx_signatures@.len()
            &&& forall|m: int| 0 <= m < sig_addrs@.len() ==>
                    #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m)
        },
{
    let n = tx_signatures.len();
    let mut sig_addrs: Vec<Vec<Addr>> = Vec::with_capacity(n);

    for i in 0..n
        invariant
            n == tx_signatures@.len(),
            sig_addrs@.len() == i,
            // Every position processed so far has correct addresses
            forall|m: int| 0 <= m < i as int ==>
                #[trigger] sig_addrs@[m]@.to_set()
                    =~= spec_addresses::<S, Addr>(tx_signatures, m),
            // No derivation failure seen yet
            forall|m: int| 0 <= m < i as int ==>
                !spec_addr_derivation_fails(tx_signatures, m),
    {
        match tx_signatures[i].try_derive_addresses() {
            Ok(addrs) => {
                proof {
                    // trait ensures: r matches Err(_) <==> spec_sig_addr_fails(self)
                    // we have Ok, so !spec_sig_addr_fails(&tx_signatures@[i])
                    assert(!spec_sig_addr_fails(&tx_signatures@[i as int]));
                    assert(!spec_addr_derivation_fails(tx_signatures, i as int));
                    // trait ensures: r->Ok_0@.to_set() == spec_sig_addresses(self)
                    //                              = spec_addresses(tx_signatures, i)
                    assert(addrs@.to_set()
                        =~= spec_addresses::<S, Addr>(tx_signatures, i as int));
                }
                sig_addrs.push(addrs);
                proof {
                    // vstd push: sig_addrs@ =~= old@ + seq![addrs]
                    // so sig_addrs@[i] == addrs
                    assert(sig_addrs@[i as int]@.to_set()
                        =~= spec_addresses::<S, Addr>(tx_signatures, i as int));
                }
            }
            Err(_) => {
                proof {
                    // trait ensures: Err <==> spec_sig_addr_fails(self)
                    assert(spec_sig_addr_fails(&tx_signatures@[i as int]));
                    assert(spec_addr_derivation_fails(tx_signatures, i as int));
                    // therefore spec_any_addr_derivation_fails holds
                    assert(spec_any_addr_derivation_fails(tx_signatures));
                }
                return Err(SigVerifyError::AddressDerivationFailed);
            }
        }
    }

    proof {
        // All positions processed with no failure → !spec_any_addr_derivation_fails
        assert(!spec_any_addr_derivation_fails(tx_signatures)) by {
            assert forall|m: int| 0 <= m < tx_signatures@.len() implies
                !spec_addr_derivation_fails(tx_signatures, m)
            by {
                // From loop invariant at i=n: forall m < n, !spec_addr_derivation_fails
            };
        };
    }

    Ok(sig_addrs)
}

// ---------------------------------------------------------------------------
// § 9  Scan helper
// ---------------------------------------------------------------------------

/// Find the first unused position j where sig j's address set intersects `aliases`
/// AND the signature is cryptographically valid for the matching address.
/// Returns `None` iff `spec_first_valid_unused` returns `None`.
///
/// Correctness note: we iterate over ALL aliases for each position (not just
/// the first common address) so that if one alias fails crypto another may
/// succeed.  This is necessary for signatures with multiple addresses (e.g.
/// zklogin legacy + canonical) and makes the proof tractable.
fn scan_first_valid<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
    sig_addrs: &[Vec<Addr>],
    aliases: &Vec<Addr>,
    epoch: u64,
    visited: &[bool],
    _ghost_used: Ghost<Set<int>>,
) -> (r: Option<(u8, Addr)>)
    requires
        tx_signatures@.len() == sig_addrs@.len(),
        tx_signatures@.len() <= u8::MAX as nat,
        visited@.len() == tx_signatures@.len(),
        forall|m: int| 0 <= m < sig_addrs@.len() ==>
            #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m),
        // Direct correspondence: visited[k] iff k is in the used set
        forall|k: int| 0 <= k < visited@.len() ==> (#[trigger] visited@[k] <==> _ghost_used@.contains(k)),
    ensures
        r.is_none() ==>
            !(spec_first_valid_unused(tx_signatures, aliases@.to_set(), epoch, _ghost_used@) matches Some(_)),
        r matches Some((j, addr)) ==> {
            &&& (j as int) < tx_signatures@.len() as int
            &&& !_ghost_used@.contains(j as int)
            &&& spec_is_valid_for(&tx_signatures@[j as int], aliases@.to_set(), epoch)
            &&& spec_first_valid_unused(tx_signatures, aliases@.to_set(), epoch, _ghost_used@) == Some(j as int)
        },
{
    let n = tx_signatures.len();

    for j in 0..n
        invariant
            n == tx_signatures@.len(),
            n == sig_addrs@.len(),
            n <= u8::MAX as nat,
            visited@.len() == n,
            forall|m: int| 0 <= m < n as int ==>
                sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m),
            // Direct correspondence: visited[k] iff k is in the used set
            forall|k: int| 0 <= k < n as int ==> (visited@[k] <==> _ghost_used@.contains(k)),
            // All positions before j are either used or not spec_is_valid_for
            forall|k: int| 0 <= k < j as int && !_ghost_used@.contains(k) ==>
                !spec_is_valid_for(&#[trigger] tx_signatures@[k], aliases@.to_set(), epoch),
    {
        // visited[j] == _ghost_used@.contains(j) (from invariant)
        if !visited[j] {
        // j is not used.  Try each alias: if it's in sig_addrs[j] AND crypto passes, return.
        let mut ai = 0usize;
        let mut found_addr: Option<Addr> = None;

        while ai < aliases.len() && found_addr.is_none()
            invariant
                ai <= aliases@.len(),
                j < n as int,
                n == tx_signatures@.len(),
                n == sig_addrs@.len(),
                // All aliases tried so far either have no address match or failed crypto.
                found_addr.is_none() ==>
                    forall|p: int| 0 <= p < ai as int ==>
                        !spec_addresses::<S, Addr>(tx_signatures, j as int).contains(#[trigger] aliases@[p])
                        || !spec_sig_crypto_valid(&tx_signatures@[j as int], aliases@[p], epoch),
                found_addr matches Some(a) ==>
                    spec_is_valid_for(&tx_signatures@[j as int], aliases@.to_set(), epoch),
                sig_addrs@[j as int]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, j as int),
            decreases aliases@.len() - ai
        {
            let a = aliases[ai];
            ai += 1;  // single increment point; branches only update found_addr
            if slice_contains(&sig_addrs[j], a) {
                match tx_signatures[j].verify_for_address(&a, epoch) {
                    Ok(()) => {
                        proof {
                            assert(spec_sig_crypto_valid(&tx_signatures@[j as int], a, epoch));
                            assert(spec_addresses::<S, Addr>(tx_signatures, j as int).contains(a));
                            assert(aliases@.to_set().contains(a));
                        }
                        found_addr = Some(a);
                    }
                    Err(_) => {
                        proof {
                            assert(!spec_sig_crypto_valid(&tx_signatures@[j as int], a, epoch));
                        }
                    }
                }
            } else {
                proof {
                    assert(!spec_addresses::<S, Addr>(tx_signatures, j as int).contains(a));
                }
            }
        }

        match found_addr {
            Some(addr) => {
                // j is the first unused valid position.
                proof {
                    lemma_first_valid_from_correct::<S, Addr>(
                        tx_signatures, aliases@.to_set(), epoch, _ghost_used@, 0, j as int,
                    );
                }
                return Some((j as u8, addr));
            }
            None => {
                // All aliases tried — none matched with valid crypto.
                proof {
                    assert(!spec_is_valid_for(&tx_signatures@[j as int], aliases@.to_set(), epoch)) by {
                        assert forall|a: Addr|
                            aliases@.to_set().contains(a)
                            implies
                            !spec_addresses::<S, Addr>(tx_signatures, j as int).contains(a)
                            || !spec_sig_crypto_valid(&tx_signatures@[j as int], a, epoch)
                        by {
                            // a ∈ aliases@.to_set() → exists p, aliases@[p] == a
                            let p = choose|p: int| 0 <= p < aliases@.len() && aliases@[p] == a;
                            // Inner loop invariant at ai == aliases@.len() covers all p < aliases@.len()
                            assert(!spec_addresses::<S, Addr>(tx_signatures, j as int).contains(aliases@[p])
                                || !spec_sig_crypto_valid(&tx_signatures@[j as int], aliases@[p], epoch));
                        };
                    };
                }
            }
        }
        } // end if !j_taken
    }

    proof {
        // Every unused position had !spec_is_valid_for → spec_first_valid_unused = None
        lemma_first_valid_from_none_invalid::<S, Addr>(
            tx_signatures, aliases@.to_set(), epoch, _ghost_used@, 0,
        );
    }
    None
}

// ---------------------------------------------------------------------------
// § 10  Recursive verified function
// ---------------------------------------------------------------------------

/// Recursive core of the greedy verification algorithm.
///
/// Processes `signers` one at a time.  `taken` (exec) / `used` (ghost) track
/// which signature positions have already been assigned to earlier senders.
/// Directly mirrors `spec_greedy_helper` so the proof is by structural induction.
fn verify_sigs_rec<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
    sig_addrs: &[Vec<Addr>],
    signers: &[(Addr, Vec<Addr>)],
    epoch: u64,
    visited: &[bool],
    used: Ghost<Set<int>>,
) -> (result: Result<Vec<u8>, SigVerifyError>)
    requires
        tx_signatures@.len() == sig_addrs@.len(),
        tx_signatures@.len() <= u8::MAX as nat,
        signers@.len() <= tx_signatures@.len() as nat,
        visited@.len() == tx_signatures@.len(),
        forall|m: int| 0 <= m < sig_addrs@.len() ==>
            #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m),
        // Direct correspondence: visited[k] iff k is in the used set
        forall|k: int| 0 <= k < visited@.len() ==> (#[trigger] visited@[k] <==> used@.contains(k)),
        // No address collision across distinct signature positions
        forall|i: int, j: int|
            0 <= i < tx_signatures@.len()
            && 0 <= j < tx_signatures@.len()
            && i != j
            ==> #[trigger] spec_addresses::<S, Addr>(tx_signatures, i)
                    .disjoint(spec_addresses::<S, Addr>(tx_signatures, j)),
    ensures
        result matches Ok(v) ==> {
            &&& spec_greedy_helper(tx_signatures, signers@, epoch, used@) matches Some(_)
            &&& v@ =~= spec_greedy_helper(tx_signatures, signers@, epoch, used@)->Some_0
            &&& v@.len() == signers@.len()
        },
        result matches Err(_) ==>
            !(spec_greedy_helper(tx_signatures, signers@, epoch, used@) matches Some(_)),
    decreases signers@.len()
{
    if signers.is_empty() {
        // Base case: spec_greedy_helper(sigs, seq![], epoch, used@) = Some(seq![])
        return Ok(vec![]);
    }

    let (_, aliases) = &signers[0];

    let scan_result = scan_first_valid(tx_signatures, sig_addrs, aliases, epoch, visited, used);

    let (j, _addr) = match scan_result {
        None => {
            proof {
                lemma_greedy_helper_none_first::<S, Addr>(tx_signatures, signers@, epoch, used@);
            }
            return Err(SigVerifyError::SignerAbsent);
        }
        Some(pair) => pair,
    };


    // Mark position j as visited for the recursive call.
    // clone_and_set ensures: result@.len() == visited@.len(), result@[j] == true,
    //                         forall k ≠ j: result@[k] == visited@[k]
    let visited_new = clone_and_set(visited, j as usize, true);
    let ghost used_new = used@.insert(j as int);

    proof {
        // visited_new[k] <==> used_new.contains(k) for all k < n
        assert forall|k: int| 0 <= k < visited_new@.len() implies
            (visited_new@[k] <==> used_new.contains(k))
        by {
            if k == j as int {
                assert(visited_new@[k] == true);
                assert(used_new.contains(j as int));
            } else {
                assert(visited_new@[k] == visited@[k]);
                assert(visited@[k] <==> used@.contains(k));
                assert(used_new.contains(k) <==> used@.contains(k) || k == j as int);
            }
        };
    }

    let rest = verify_sigs_rec(
        tx_signatures,
        sig_addrs,
        &signers[1..signers.len()],
        epoch,
        &visited_new,
        Ghost(used_new),
    )?;

    // Build result = [j] + rest.  prepend_u8 ensures result@ =~= seq![j] + rest@.
    let result = prepend_u8(j, &rest);

    proof {
        // Apply lemma_greedy_helper_unfold
        lemma_greedy_helper_unfold::<S, Addr>(
            tx_signatures, signers@, epoch, used@, j as int, rest@,
        );
        // spec_greedy_helper == Some(seq![j as u8] + rest@) == Some(result@)
        assert(spec_greedy_helper(tx_signatures, signers@, epoch, used@) == Some(result@));
        // result@.len() == signers@.len()
        assert(result@.len() == signers@.len() as nat) by {
            assert(result@.len() == 1 + rest@.len());
            assert(rest@.len() == signers@.skip(1).len() as nat);
            assert(signers@.skip(1).len() == signers@.len() - 1);
        };
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// § 11  Aliased-signers preprocessing
// ---------------------------------------------------------------------------

/// The alias set for a single `signer`: the first matching entry's alias list
/// if one exists in `aliased`, otherwise just `[signer]` (the typical case).
pub open spec fn lookup_aliases<Addr>(
    signer: Addr,
    aliased: Seq<(Addr, NonEmpty<Addr>)>,
) -> Seq<Addr>
    decreases aliased.len()
{
    if aliased.len() == 0 {
        seq![signer]
    } else if aliased[0].0 == signer {
        nonempty_view(&aliased[0].1)
    } else {
        lookup_aliases(signer, aliased.skip(1))
    }
}

/// Look up the alias set for `signer` in `aliased_addresses`.
/// Returns `[signer]` if no entry matches (the typical case).
///
/// Trusted (`external_body`) — NonEmpty<T>'s internals and tuple-field access
/// through slice indexing aren't cleanly tracked in Verus.  The spec captures
/// exactly what callers need via `lookup_aliases`.
#[verifier::external_body]
fn find_aliases_for_signer<Addr: PartialEq + Eq + Copy>(
    signer: Addr,
    aliased_addresses: &[(Addr, NonEmpty<Addr>)],
) -> (result: Vec<Addr>)
    ensures
        result@ == lookup_aliases::<Addr>(signer, aliased_addresses@),
{
    aliased_addresses
        .iter()
        .find(|(addr, _)| *addr == signer)
        .map(|(_, a)| a.iter().cloned().collect())
        .unwrap_or_else(|| vec![signer])
}

/// Build the `(canonical_sender, aliases)` pairs that `verify_signatures` expects.
///
/// For each signer, looks up its alias set in `aliased_addresses`; if absent,
/// the alias set defaults to `[signer]`.
pub fn build_required_signers<Addr: PartialEq + Eq + Copy>(
    signers: &[Addr],
    aliased_addresses: &[(Addr, NonEmpty<Addr>)],
) -> (result: Vec<(Addr, Vec<Addr>)>)
    ensures
        result@.len() == signers@.len(),
        forall|k: int| 0 <= k < signers@.len() ==>
            (#[trigger] result@[k].0 == signers@[k])
            && result@[k].1@ == lookup_aliases::<Addr>(signers@[k], aliased_addresses@),
{
    let n = signers.len();
    let mut result: Vec<(Addr, Vec<Addr>)> = Vec::with_capacity(n);
    let mut i = 0usize;
    while i < n
        invariant
            n == signers@.len(),
            i <= n,
            result@.len() == i,
            forall|k: int| 0 <= k < i as int ==>
                (#[trigger] result@[k].0 == signers@[k])
                && result@[k].1@ == lookup_aliases::<Addr>(signers@[k], aliased_addresses@),
        decreases n - i
    {
        let signer = signers[i];
        let aliases = find_aliases_for_signer(signer, aliased_addresses);
        result.push((signer, aliases));
        i += 1;
    }
    result
}

// ---------------------------------------------------------------------------
// § 12  Public entry point
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
            result matches Err(_)
        } else if spec_any_addr_derivation_fails(tx_signatures) {
            result matches Err(_)
        } else if !(spec_greedy_result(tx_signatures, required_signers, epoch) matches Some(_)) {
            result matches Err(_)
        } else {
            &&& result matches Ok(_)
            &&& ok_indices(&result)
                    =~= spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0
        },
        result matches Ok(_) ==> ok_indices(&result).len() == required_signers@.len(),
        result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> (ok_indices(&result)[k] as int) < tx_signatures@.len(),
        result matches Ok(_) ==>
            forall|k1: int, k2: int|
                0 <= k1 < ok_indices(&result).len()
                && 0 <= k2 < ok_indices(&result).len()
                && k1 != k2
                ==> ok_indices(&result)[k1] != ok_indices(&result)[k2],
        result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> #[trigger] spec_is_valid_for(
                        &tx_signatures@[ok_indices(&result)[k] as int],
                        required_signers@[k].1@.to_set(),
                        epoch,
                    ),
{
    let n = tx_signatures.len();

    // E1: count mismatch
    if n != required_signers.len() {
        return Err(SigVerifyError::SignerCountMismatch {
            actual: n,
            expected: required_signers.len(),
        });
    }

    // E2: derive addresses; fails immediately on malformed signatures.
    let sig_addrs = match derive_all_addresses(tx_signatures) {
        Ok(a) => a,
        Err(e) => return Err(e),
    };


    // Delegate to the recursive greedy implementation.
    // All positions start unvisited; `used` starts as Set::empty().
    // verify_sigs_rec directly mirrors spec_greedy_helper.
    proof {
        // derive_all_addresses Ok → sig_addrs@.len() == tx_signatures@.len() == n
        assert(sig_addrs@.len() == n as int);
        // derive_all_addresses Ok → !spec_any_addr_derivation_fails
        assert(!spec_any_addr_derivation_fails(tx_signatures));
        // With no failures, the conditional disjoint precondition becomes unconditional.
        // verify_signatures requires: i != j && !fails(i) && !fails(j) ==> disjoint.
        // Since !fails holds for all, the guards are always true.
        assert(forall|i: int, j: int|
            0 <= i < tx_signatures@.len() && 0 <= j < tx_signatures@.len() && i != j
            ==> spec_addresses::<S, Addr>(tx_signatures, i)
                    .disjoint(spec_addresses::<S, Addr>(tx_signatures, j))) by {
            assert forall|i: int, j: int|
                0 <= i < tx_signatures@.len() && 0 <= j < tx_signatures@.len() && i != j
                implies spec_addresses::<S, Addr>(tx_signatures, i)
                        .disjoint(spec_addresses::<S, Addr>(tx_signatures, j))
            by {
                // !spec_any_addr_derivation_fails means !spec_addr_derivation_fails for all k
                assert(!spec_addr_derivation_fails(tx_signatures, i));
                assert(!spec_addr_derivation_fails(tx_signatures, j));
                // These are exactly the guards in verify_signatures requires
            };
        };
    }
    // visited starts as all-false (no positions used yet)
    // required_signers@.len() == n == tx_signatures@.len() (count check passed)
    let visited_init = vec![false; n];
    let rec_result = verify_sigs_rec(
        tx_signatures,
        &sig_addrs,
        required_signers,
        epoch,
        &visited_init,
        Ghost(Set::empty()),
    );

    proof {
        assert(tx_signatures@.len() == required_signers@.len() as nat);
        // derive_all_addresses Ok ensures !spec_any_addr_derivation_fails
        assert(!spec_any_addr_derivation_fails(tx_signatures));

        // spec_greedy_result = spec_greedy_helper(..., Set::empty())
        // verify_sigs_rec was called with empty taken/used, so its ensures connect directly.

        // tx_signatures@.len() == required_signers@.len() <= u8::MAX
        assert(tx_signatures@.len() <= u8::MAX as nat);

        // Apply challenge theorem lemmas to get CT1-CT4 for Set::empty()
        lemma_greedy_len::<S, Addr>(tx_signatures, required_signers@, epoch, Set::empty());
        lemma_greedy_bounds::<S, Addr>(tx_signatures, required_signers@, epoch, Set::empty());
        lemma_greedy_not_in_used_and_distinct::<S, Addr>(tx_signatures, required_signers@, epoch, Set::empty());
        lemma_greedy_valid::<S, Addr>(tx_signatures, required_signers@, epoch, Set::empty());

        // Connect verify_sigs_rec Ok result to spec_greedy_result
        // verify_sigs_rec ensures: Ok(v) ==> spec_greedy_helper(..., Set::empty()) = Some(v@)
        // spec_greedy_result(sigs, signers, epoch) = spec_greedy_helper(sigs, signers@, epoch, Set::empty())
        // R1: ok result matches spec_greedy_result
        if rec_result matches Ok(_) {
            let v = rec_result->Ok_0;
            assert(spec_greedy_helper(tx_signatures, required_signers@, epoch, Set::empty()) matches Some(_));
            assert(v@ =~= spec_greedy_helper(tx_signatures, required_signers@, epoch, Set::empty())->Some_0);
            assert(spec_greedy_result(tx_signatures, required_signers, epoch) matches Some(_));
            assert(ok_indices(&rec_result) =~= spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0);

            // R1: ok_indices.len() == required_signers@.len()
            // From lemma_greedy_len: Some ==> len == signers.len()
            assert(ok_indices(&rec_result).len() == required_signers@.len());

            // R2: ok_indices[k] < tx_signatures.len() for all k
            // From lemma_greedy_bounds
            assert(forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                ==> (ok_indices(&rec_result)[k] as int) < tx_signatures@.len()) by {
                let indices = spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0;
                assert forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                    implies (ok_indices(&rec_result)[k] as int) < tx_signatures@.len()
                by {
                    assert(ok_indices(&rec_result)[k] == indices[k]);
                };
            };

            // R3: ok_indices pairwise distinct
            // From lemma_greedy_not_in_used_and_distinct
            assert(forall|k1: int, k2: int|
                0 <= k1 < ok_indices(&rec_result).len()
                && 0 <= k2 < ok_indices(&rec_result).len()
                && k1 != k2
                ==> ok_indices(&rec_result)[k1] != ok_indices(&rec_result)[k2]) by {
                let indices = spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0;
                assert forall|k1: int, k2: int|
                    0 <= k1 < ok_indices(&rec_result).len()
                    && 0 <= k2 < ok_indices(&rec_result).len()
                    && k1 != k2
                    implies ok_indices(&rec_result)[k1] != ok_indices(&rec_result)[k2]
                by {
                    assert(ok_indices(&rec_result)[k1] == indices[k1]);
                    assert(ok_indices(&rec_result)[k2] == indices[k2]);
                };
            };

            // R4: spec_is_valid_for at each index
            // From lemma_greedy_valid
            assert(forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                ==> spec_is_valid_for(
                        &tx_signatures@[ok_indices(&rec_result)[k] as int],
                        required_signers@[k].1@.to_set(),
                        epoch,
                    )) by {
                let indices = spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0;
                assert forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                    implies spec_is_valid_for(
                            &tx_signatures@[ok_indices(&rec_result)[k] as int],
                            required_signers@[k].1@.to_set(),
                            epoch,
                        )
                by {
                    // R2 ensures ok_indices[k] < tx_signatures.len() (bounds check)
                    assert((ok_indices(&rec_result)[k] as int) < tx_signatures@.len() as int);
                    assert(ok_indices(&rec_result)[k] == indices[k]);
                    // Trigger lemma_greedy_valid's forall: spec_greedy_helper(...)->Some_0[k]
                    assert(spec_is_valid_for(
                        &tx_signatures@[indices[k] as int],
                        required_signers@[k].1@.to_set(),
                        epoch,
                    ));
                };
            };
        }
    }
    rec_result
}

} // verus!
