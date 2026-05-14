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
pub trait SignatureVerifiable<Addr> {
    /// Derive all addresses this signature is associated with.
    ///
    /// A single signature may yield more than one address (e.g. a zklogin
    /// signature with legacy-address support). Returns
    /// `Err(SigVerifyError::AddressDerivationFailed)` for malformed input.
    fn try_derive_addresses(&self) -> (r: Result<Vec<Addr>, SigVerifyError>);

    /// Cryptographically verify this signature as proof of authorization by
    /// `addr` at `epoch`. Returns `Err(CryptoVerificationFailed)` on failure.
    fn verify_for_address(&self, addr: &Addr, epoch: u64) -> (r: Result<(), SigVerifyError>);
}

// ---------------------------------------------------------------------------
// § 3  Abstract spec predicates (single-element primitives)
// ---------------------------------------------------------------------------
// These operate on a single `&S` so they can be connected to the
// `SignatureVerifiable` trait methods via `assume_specification`.

/// The set of addresses derivable from a single signature.
/// Undefined (and never queried) when `spec_sig_addr_fails(sig)`.
pub uninterp spec fn spec_sig_addresses<S, Addr>(sig: &S) -> Set<Addr>;

/// Whether address derivation fails for a single signature.
pub uninterp spec fn spec_sig_addr_fails<S>(sig: &S) -> bool;

/// Whether a single signature is cryptographically valid for `addr` at `epoch`.
/// Independent of aliases — this is the raw crypto check.
pub uninterp spec fn spec_sig_crypto_valid<S, Addr>(sig: &S, addr: Addr, epoch: u64) -> bool;

/// The alias set for `sender` given the `aliased_addresses` mapping.
/// Defaults to `{sender}` when `sender` is not a key in the mapping.
pub uninterp spec fn spec_aliases<Addr>(
    sender: Addr,
    aliased_addresses: &[(Addr, Vec<Addr>)],
) -> Set<Addr>;

// ---------------------------------------------------------------------------
// § 4  Derived spec predicates (slice-indexed, used in ensures clauses)
// ---------------------------------------------------------------------------
// These lift the single-element primitives to slice + index form for use
// in the function postconditions which reason about `&[S]` inputs.

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

/// Whether the signature at position `i` is valid for `sender`:
///   - there exists an address A in both addresses(sigs[i]) and aliases(sender), AND
///   - sigs[i] is cryptographically valid for A at `epoch`.
///
/// Verification runs against the matching address A, not against `sender` directly.
pub open spec fn spec_is_valid_for<S, Addr>(
    sigs: &[S],
    i: int,
    sender: Addr,
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
) -> bool {
    exists|a: Addr|
        #![trigger spec_addresses::<S, Addr>(sigs, i).contains(a)]
        spec_addresses::<S, Addr>(sigs, i).contains(a)
        && spec_aliases(sender, aliased_addresses).contains(a)
        && spec_sig_crypto_valid(&sigs@[i], a, epoch)
}

/// The greedy assignment: for each sender k (in order), the index of the first
/// unused position j in `sigs` such that `spec_is_valid_for(sigs, j, signers[k], ...)`.
/// Returns `None` if no such position exists for any sender.
pub open spec fn spec_greedy_result<S, Addr>(
    sigs: &[S],
    required_signers: &[Addr],
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
) -> Option<Seq<u8>>
    decreases required_signers@.len()
{
    spec_greedy_helper(sigs, required_signers@, aliased_addresses, epoch, Set::empty())
}

/// Recursive helper for the greedy algorithm.
///
/// `used` tracks which positions have already been assigned to earlier senders.
/// At each step, finds the smallest j ∉ used with `spec_is_valid_for(sigs, j, signers[0], ...)`.
pub open spec fn spec_greedy_helper<S, Addr>(
    sigs: &[S],
    signers: Seq<Addr>,
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
    used: Set<int>,
) -> Option<Seq<u8>>
    decreases signers.len()
{
    if signers.len() == 0 {
        Some(seq![])
    } else {
        // Find the first unused position valid for signers[0].
        let j = spec_first_valid_unused(sigs, signers[0], aliased_addresses, epoch, used);
        match j {
            None => None,
            Some(j) => {
                match spec_greedy_helper(sigs, signers.skip(1), aliased_addresses, epoch, used.insert(j)) {
                    None => None,
                    Some(rest) => Some(seq![j as u8] + rest),
                }
            }
        }
    }
}

/// The smallest position j in `sigs` that is (a) not in `used` and
/// (b) valid for `sender`. Returns `None` if no such position exists.
pub open spec fn spec_first_valid_unused<S, Addr>(
    sigs: &[S],
    sender: Addr,
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
    used: Set<int>,
) -> Option<int>
    decreases sigs@.len()
{
    spec_first_valid_unused_from(sigs, sender, aliased_addresses, epoch, used, 0)
}

pub open spec fn spec_first_valid_unused_from<S, Addr>(
    sigs: &[S],
    sender: Addr,
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
    used: Set<int>,
    start: int,
) -> Option<int>
    decreases sigs@.len() - start
{
    if start >= sigs@.len() {
        None
    } else if !used.contains(start) && spec_is_valid_for(sigs, start, sender, aliased_addresses, epoch) {
        Some(start)
    } else {
        spec_first_valid_unused_from(sigs, sender, aliased_addresses, epoch, used, start + 1)
    }
}

// ---------------------------------------------------------------------------
// § 5  Helper: extract Ok value
// ---------------------------------------------------------------------------

spec fn ok_indices(result: &Result<Vec<u8>, SigVerifyError>) -> Seq<u8> {
    match result {
        Ok(v) => v@,
        Err(_) => seq![],
    }
}

// ---------------------------------------------------------------------------
// § 6  Verified function (body left unimplemented — proof to follow)
// ---------------------------------------------------------------------------

/// Verify signatures on a user-signed transaction.
///
/// The caller is responsible for:
/// - Checking that the transaction intent is `SUI_TRANSACTION_INTENT`.
/// - Skipping this call for system transactions (which are unconditionally valid).
///
/// # Preconditions
/// - `required_signers.len() <= 255` (indices stored as `u8`).
/// - For all distinct positions i ≠ j in `tx_signatures`,
///   `addresses(tx_signatures[i]) ∩ addresses(tx_signatures[j]) = {}`.
///   (No address collision — required for the greedy algorithm to be correct.)
///
/// # Contract
///
/// See `crates/sui-types/verify_sig_spec.md`.
///
/// Returns `Ok(indices)` where `indices[k]` is the position of the first
/// unused valid signature for `required_signers[k]` (greedy, in sender order).
/// Returns `Err` if any validity condition is violated.
#[verifier::external_body]
pub fn verify_signatures<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    tx_signatures: &[S],
    required_signers: &[Addr],
    epoch: u64,
    aliased_addresses: &[(Addr, Vec<Addr>)],
) -> (result: Result<Vec<u8>, SigVerifyError>)
    requires
        required_signers@.len() <= u8::MAX as nat,
        // No address collision across distinct signature positions.
        forall|i: int, j: int|
            0 <= i < tx_signatures@.len()
            && 0 <= j < tx_signatures@.len()
            && i != j
            && !spec_addr_derivation_fails(tx_signatures, i)
            && !spec_addr_derivation_fails(tx_signatures, j)
            ==> #[trigger] spec_addresses::<S, Addr>(tx_signatures, i)
                    .disjoint(spec_addresses::<S, Addr>(tx_signatures, j)),
    ensures
        // Complete case analysis — the else implicitly excludes all prior conditions.
        if tx_signatures@.len() != required_signers@.len() {
            result.is_Err()                                          // E1: count mismatch
        } else if spec_any_addr_derivation_fails(tx_signatures) {
            result.is_Err()                                          // E2: derivation failure
        } else if spec_greedy_result(
            tx_signatures, required_signers, aliased_addresses, epoch,
        ).is_None() {
            result.is_Err()                                          // E3: greedy fails
        } else {
            &&& result.is_Ok()                                       // S1: greedy succeeds
            &&& ok_indices(&result)
                    =~= spec_greedy_result(
                            tx_signatures, required_signers, aliased_addresses, epoch,
                        ).get_Some_0()
        },
        // Return value invariants (stated separately for callers; derivable from S1 + greedy spec).
        // R1: length matches required_signers
        result.is_Ok() ==> ok_indices(&result).len() == required_signers@.len(),
        // R2: every index is in bounds
        result.is_Ok() ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> (ok_indices(&result)[k] as int) < tx_signatures@.len(),
        // R3: indices are pairwise distinct (bijection — no sig position used twice)
        result.is_Ok() ==>
            forall|k1: int, k2: int|
                0 <= k1 < ok_indices(&result).len()
                && 0 <= k2 < ok_indices(&result).len()
                && k1 != k2
                ==> ok_indices(&result)[k1] != ok_indices(&result)[k2],
        // R4: each assigned signature is valid for its required signer
        result.is_Ok() ==>
            forall|k: int| 0 <= k < ok_indices(&result).len()
                ==> #[trigger] spec_is_valid_for(
                        tx_signatures,
                        ok_indices(&result)[k] as int,
                        required_signers@[k],
                        aliased_addresses,
                        epoch,
                    ),
{
    unimplemented!()
}

} // verus!
