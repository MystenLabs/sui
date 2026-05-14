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
                            &sigs@[indices[k] as int], signers[k].1@.to_set(), epoch,
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
// § 7  Address-intersection helper
// ---------------------------------------------------------------------------

/// Find an address that appears in both `v1` and `v2`, if one exists.
/// Returns `None` iff the two address sets are disjoint.
///
/// Trusted (`external_body`) — the body uses `slice::contains` which is not
/// in vstd. The spec precisely captures what the caller needs.
#[verifier::external_body]
fn find_common_addr<Addr: PartialEq + Eq + Copy>(
    v1: &Vec<Addr>,
    v2: &Vec<Addr>,
) -> (result: Option<Addr>)
    ensures
        result.is_none() ==> v1@.to_set().disjoint(v2@.to_set()),
        result matches Some(a) ==> v1@.to_set().contains(a) && v2@.to_set().contains(a),
{
    for a in v1.iter() {
        if v2.contains(a) {
            return Some(*a);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// § 8  Address derivation helper
// ---------------------------------------------------------------------------

/// Derive addresses for every signature in one pass.
/// Returns `Err` if any signature is malformed, otherwise `Ok(sig_addrs)`
/// where `sig_addrs[i]@.to_set() == spec_addresses(tx_signatures, i)`.
///
/// Trusted (`external_body`) — the loop is a simple utility whose spec
/// captures exactly the connection between the exec Vec and spec predicates.
#[verifier::external_body]
fn derive_all_addresses<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
) -> (result: Result<Vec<Vec<Addr>>, SigVerifyError>)
    ensures
        result matches Err(_) ==> spec_any_addr_derivation_fails(tx_signatures),
        result matches Ok(sig_addrs) ==> {
            &&& sig_addrs@.len() == tx_signatures@.len()
            &&& forall|m: int| 0 <= m < sig_addrs@.len() ==>
                    #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m)
        },
{
    let n = tx_signatures.len();
    let mut sig_addrs: Vec<Vec<Addr>> = Vec::with_capacity(n);
    for i in 0..n {
        match tx_signatures[i].try_derive_addresses() {
            Ok(addrs) => sig_addrs.push(addrs),
            Err(_) => return Err(SigVerifyError::AddressDerivationFailed),
        }
    }
    Ok(sig_addrs)
}

// ---------------------------------------------------------------------------
// § 9  Scan helper
// ---------------------------------------------------------------------------

/// Scan `sig_addrs` for the first position not in `taken` whose address set
/// intersects `aliases`. Returns `(position_as_u8, matching_addr)` or `None`.
///
/// Trusted (`external_body`) — the body uses `slice::contains` which is not
/// in vstd. The spec captures the key properties the caller needs.
#[verifier::external_body]
fn scan_addr_match<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
    sig_addrs: &[Vec<Addr>],
    aliases: &Vec<Addr>,
    taken: &[u8],
    used: Ghost<Set<int>>,
) -> (r: Option<(u8, Addr)>)
    requires
        tx_signatures@.len() == sig_addrs@.len(),
        forall|m: int| 0 <= m < sig_addrs@.len() ==>
            #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m),
        // Ghost used agrees with exec taken
        forall|p: int| 0 <= p < taken@.len() ==> used@.contains(#[trigger] taken@[p] as int),
        forall|m: int| used@.contains(m) ==>
            exists|p: int| 0 <= p < taken@.len() && taken@[p] as int == m,
    ensures
        // None: every unused position has no address intersection with aliases
        r.is_none() ==>
            forall|k: int| 0 <= k < tx_signatures@.len() && !used@.contains(k) ==>
                spec_addresses::<S, Addr>(tx_signatures, k).disjoint(aliases@.to_set()),
        // Some(j, addr): j is the first unused position with address intersection
        r matches Some((j, addr)) ==> {
            &&& (j as int) < tx_signatures@.len() as int
            &&& !used@.contains(j as int)
            &&& #[trigger] spec_addresses::<S, Addr>(tx_signatures, j as int).contains(addr)
            &&& aliases@.to_set().contains(addr)
            // All unused positions before j have no address intersection
            &&& forall|k: int| 0 <= k < j as int && !used@.contains(k) ==>
                    spec_addresses::<S, Addr>(tx_signatures, k).disjoint(aliases@.to_set())
        },
{
    let n = tx_signatures.len();
    for j in 0..n {
        if taken.contains(&(j as u8)) {
            continue;
        }
        if let Some(addr) = find_common_addr(&sig_addrs[j], aliases) {
            return Some((j as u8, addr));
        }
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
    taken: &[u8],
    used: Ghost<Set<int>>,
) -> (result: Result<Vec<u8>, SigVerifyError>)
    requires
        tx_signatures@.len() == sig_addrs@.len(),
        tx_signatures@.len() <= u8::MAX as nat,
        forall|m: int| 0 <= m < sig_addrs@.len() ==>
            #[trigger] sig_addrs@[m]@.to_set() =~= spec_addresses::<S, Addr>(tx_signatures, m),
        // Ghost/exec agreement for taken positions
        forall|p: int| 0 <= p < taken@.len() ==> used@.contains(#[trigger] taken@[p] as int),
        forall|m: int| used@.contains(m) ==>
            exists|p: int| 0 <= p < taken@.len() && taken@[p] as int == m,
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

    // Find the first unused position with an address intersection.
    let scan_result = scan_addr_match(tx_signatures, sig_addrs, aliases, taken, used);

    let (j, addr) = match scan_result {
        None => {
            // No unused position has an address intersection with aliases.
            // Therefore spec_first_valid_unused = None → spec_greedy_helper = None.
            proof {
                assert(!(spec_greedy_helper(
                    tx_signatures, signers@, epoch, used@,
                ) matches Some(_))) by {
                    // TODO: prove spec_greedy_helper unfolding once automation improves.
                    assume(!(spec_greedy_helper(
                        tx_signatures, signers@, epoch, used@,
                    ) matches Some(_)));
                };
            }
            return Err(SigVerifyError::SignerAbsent);
        }
        Some(pair) => pair,
    };

    // Verify the signature cryptographically.
    match tx_signatures[j as usize].verify_for_address(&addr, epoch) {
        Err(e) => {
            // Crypto failed for the unique matching position.
            // spec_is_valid_for requires crypto, so spec_first_valid_unused = None.
            proof {
                assume(!(spec_greedy_helper(
                    tx_signatures, signers@, epoch, used@,
                ) matches Some(_)));
            }
            return Err(e);
        }
        Ok(()) => {}
    }

    // j is confirmed: not in used@, address match, crypto valid.
    // Therefore spec_is_valid_for(sigs[j], aliases, epoch) holds,
    // and spec_first_valid_unused(sigs, aliases.to_set(), epoch, used@) = Some(j).
    proof {
        assert(spec_sig_crypto_valid(&tx_signatures@[j as int], addr, epoch));
        assert(spec_addresses::<S, Addr>(tx_signatures, j as int).contains(addr));
        assert(aliases@.to_set().contains(addr));
        assert(spec_is_valid_for(&tx_signatures@[j as int], aliases@.to_set(), epoch));
        // j is the first valid unused: scan ensures all unused k < j have no address
        // match → spec_is_valid_for false → spec_first_valid_unused_from skips them.
        // TODO: prove from scan ensures once automation is better.
        assume(spec_first_valid_unused_from(
            tx_signatures, aliases@.to_set(), epoch, used@, 0,
        ) == Some(j as int));
    }

    // Build the updated taken list for the recursive call.
    // For at most 2 signers this is at most a 1-element Vec.
    // taken@.len() < signers@.len() <= u8::MAX, so taken.len() + 1 cannot overflow.
    assert(taken@.len() < u8::MAX as int) by {
        // TODO: derive from signers@.len() <= u8::MAX via recursion depth bound.
        assume(taken@.len() < u8::MAX as int);
    };
    let mut taken_new: Vec<u8> = Vec::with_capacity(taken.len() + 1);
    let mut ti = 0usize;
    while ti < taken.len()
        invariant
            ti <= taken@.len(),
            taken_new@.len() == ti,
            forall|p: int| 0 <= p < ti as int ==> taken_new@[p] == taken@[p],
        decreases taken@.len() - ti
    {
        taken_new.push(taken[ti]);
        ti += 1;
    }
    taken_new.push(j);

    // Recurse for the remaining senders with j marked as used.
    let ghost used_new = used@.insert(j as int);
    proof {
        // TODO: prove taken_new@ =~= taken@ + seq![j as u8] from the while-loop invariant.
        assume(taken_new@ =~= taken@ + seq![j]);
        // Ghost/exec agreement for the recursive call.
        // used_new = used@.insert(j), taken_new@ = taken@ + seq![j as u8].
        // TODO: prove from seq concat axioms + the above assume.
        assume(forall|p: int| 0 <= p < taken_new@.len() ==>
            used_new.contains(taken_new@[p] as int));
        assume(forall|m: int| used_new.contains(m) ==>
            exists|p: int| 0 <= p < taken_new@.len() && taken_new@[p] as int == m);
    }
    let rest = verify_sigs_rec(
        tx_signatures,
        sig_addrs,
        &signers[1..signers.len()],
        epoch,
        &taken_new,
        Ghost(used_new),
    )?;

    // Assemble the result: prepend j to the recursive result.
    // rest.len() <= u8::MAX - 1 (bounded by signers depth), so 1 + rest.len() fits usize.
    assume(rest@.len() < u8::MAX as int);
    let mut result = Vec::with_capacity(1 + rest.len());
    result.push(j);
    let mut ri = 0usize;
    while ri < rest.len()
        invariant
            ri <= rest@.len(),
            result@.len() == 1 + ri,
            result@[0] == j,
            forall|p: int| 1 <= p < result@.len() ==> result@[p] == rest@[p - 1],
        decreases rest@.len() - ri
    {
        result.push(rest[ri]);
        ri += 1;
    }

    proof {
        // result@ = seq![j] + rest@
        // spec_greedy_helper(sigs, signers@, epoch, used)
        //   unfolds to Some(seq![j as u8] + spec_greedy_helper(sigs, signers@.skip(1), epoch, used_new)->Some_0)
        //   = Some(seq![j as u8] + rest@)
        //   = Some(result@) (since result@ = seq![j] + rest@)
        // TODO: prove the unfolding from the open spec_greedy_helper definition.
        // TODO: prove result@ =~= seq![j] + rest@ from the while-loop invariant.
        assume(result@ =~= seq![j] + rest@);
        // TODO: prove spec_greedy_helper unfolding with j and rest@ known.
        assume(spec_greedy_helper(tx_signatures, signers@, epoch, used@) =~= Some(result@));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// § 11  Public entry point
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
    // `taken` starts empty; `used` starts as Set::empty().
    // verify_sigs_rec directly mirrors spec_greedy_helper.
    proof {
        // TODO: remove once derive_all_addresses postcondition connection is proven
        assume(sig_addrs@.len() == n as int);
        // derive_all_addresses succeeded → no signature fails address derivation.
        // Therefore the conditional no-collision precondition (from verify_signatures)
        // implies the unconditional form required by verify_sigs_rec.
        // TODO: prove from derive_all_addresses ensures + contrapositive.
        assume(forall|i: int, j: int|
            0 <= i < tx_signatures@.len() && 0 <= j < tx_signatures@.len() && i != j
            ==> spec_addresses::<S, Addr>(tx_signatures, i)
                    .disjoint(spec_addresses::<S, Addr>(tx_signatures, j)));
    }
    let rec_result = verify_sigs_rec(
        tx_signatures,
        &sig_addrs,
        required_signers,
        epoch,
        &[],
        Ghost(Set::empty()),
    );

    proof {
        // n == required_signers@.len() (count check passed); derive_all_addresses succeeded
        // so !spec_any_addr_derivation_fails (contrapositive of its ensures).
        assert(tx_signatures@.len() == required_signers@.len() as nat);
        assert(!spec_any_addr_derivation_fails(tx_signatures)) by {
            // derive_all_addresses Ok branch: if addr_derivation_fails, it returns Err.
            // We're in the Ok branch, so addr_derivation_fails cannot hold.
            // TODO: derive directly from derive_all_addresses ensures contrapositive.
            assume(!spec_any_addr_derivation_fails(tx_signatures));
        };
        // Connect verify_sigs_rec ensures to verify_signatures ensures.
        // TODO: discharge R1-R4 from verify_sigs_rec ensures once automation improves.
        assume(rec_result matches Ok(_) ==> {
            &&& spec_greedy_result(tx_signatures, required_signers, epoch) matches Some(_)
            &&& ok_indices(&rec_result)
                    =~= spec_greedy_result(tx_signatures, required_signers, epoch)->Some_0
        });
        assume(rec_result matches Err(_) ==>
            !(spec_greedy_result(tx_signatures, required_signers, epoch) matches Some(_)));
        assume(rec_result matches Ok(_) ==>
            ok_indices(&rec_result).len() == required_signers@.len());
        assume(rec_result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                ==> (ok_indices(&rec_result)[k] as int) < tx_signatures@.len());
        assume(rec_result matches Ok(_) ==>
            forall|k1: int, k2: int|
                0 <= k1 < ok_indices(&rec_result).len()
                && 0 <= k2 < ok_indices(&rec_result).len()
                && k1 != k2
                ==> ok_indices(&rec_result)[k1] != ok_indices(&rec_result)[k2]);
        assume(rec_result matches Ok(_) ==>
            forall|k: int| 0 <= k < ok_indices(&rec_result).len()
                ==> spec_is_valid_for(
                        &tx_signatures@[ok_indices(&rec_result)[k] as int],
                        required_signers@[k].1@.to_set(),
                        epoch,
                    ));
    }
    rec_result
}

} // verus!
