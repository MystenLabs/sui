// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Formally verified signature verification for sender-signed transactions.
//!
//! The function [`verify_signatures`] is generic over the signature and address
//! types so that this crate has no dependency on `sui-types`. The concrete
//! instantiation (`GenericSignature`, `SuiAddress`) lives in `sui-types`.
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
    /// The transaction intent is not `SUI_TRANSACTION_INTENT`.
    WrongIntent,
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
// § 3  Spec predicates
// ---------------------------------------------------------------------------
// All uninterpreted — concrete interpretations are provided via
// `assume_specification` on the `SignatureVerifiable` impl methods during
// proof development.

/// Whether any signature in `sigs` has an uncomputable address set.
pub uninterp spec fn spec_any_addr_derivation_fails<S>(sigs: &[S]) -> bool;

/// Result of the greedy assignment algorithm (see informal spec).
///
/// `Some(indices)` — every required signer was matched; `indices[k]` is the
/// first unused position in `sigs` that is valid for `required_signers[k]`.
/// `None` — the algorithm failed for at least one required signer.
pub uninterp spec fn spec_greedy_result<S, Addr>(
    sigs: &[S],
    required_signers: &[Addr],
    aliased_addresses: &[(Addr, Vec<Addr>)],
    epoch: u64,
) -> Option<Seq<u8>>;

// Helper: extracts the byte sequence from an Ok result, or empty on Err.
spec fn ok_indices(result: &Result<Vec<u8>, SigVerifyError>) -> Seq<u8> {
    match result {
        Ok(v) => v@,
        Err(_) => seq![],
    }
}

// ---------------------------------------------------------------------------
// § 4  Verified function (body left unimplemented — proof to follow)
// ---------------------------------------------------------------------------

/// Verify signatures on a sender-signed transaction.
///
/// # Parameters
/// - `intent_is_sui_tx` — whether the transaction's intent is
///   `SUI_TRANSACTION_INTENT` (checked by the caller before this call).
/// - `tx_signatures` — ordered sequence of authenticators.
/// - `required_signers` — ordered sequence of addresses that must sign.
/// - `is_system_tx` — system transactions are unconditionally valid.
/// - `epoch` — current epoch, used for cryptographic verification.
/// - `aliased_addresses` — maps each canonical sender to its valid signing
///   aliases; a sender not in this list is its own sole alias.
///
/// # Contract
///
/// See `crates/sui-types/verify_sig_spec.md`.
///
/// Briefly: returns `Ok(indices)` where `indices[k]` is the position of the
/// first unused valid signature for `required_signers[k]`, or `Err` if any
/// validity condition is violated.
#[verifier::external_body]
pub fn verify_signatures<S: SignatureVerifiable<Addr>, Addr: PartialEq + Eq + Copy>(
    intent_is_sui_tx: bool,
    tx_signatures: &[S],
    required_signers: &[Addr],
    is_system_tx: bool,
    epoch: u64,
    aliased_addresses: &[(Addr, Vec<Addr>)],
) -> (result: Result<Vec<u8>, SigVerifyError>)
    requires
        // Indices are stored as u8; the caller must ensure the signer count fits.
        required_signers@.len() <= u8::MAX as nat,
    ensures
        // E1: wrong intent → Err
        !intent_is_sui_tx ==> result.is_Err(),
        // E2: count mismatch (user tx) → Err
        (!is_system_tx && tx_signatures@.len() != required_signers@.len())
            ==> result.is_Err(),
        // E3: address derivation failure → Err
        spec_any_addr_derivation_fails(tx_signatures) ==> result.is_Err(),
        // S1: system tx (no derivation failure) → Ok with sequential indices
        (intent_is_sui_tx
            && is_system_tx
            && !spec_any_addr_derivation_fails(tx_signatures)) ==> {
            &&& result.is_Ok()
            &&& ok_indices(&result) =~= Seq::new(required_signers@.len(), |i: int| i as u8)
        },
        // E4: greedy fails (user tx) → Err
        (intent_is_sui_tx
            && !is_system_tx
            && !spec_any_addr_derivation_fails(tx_signatures)
            && tx_signatures@.len() == required_signers@.len()
            && spec_greedy_result(tx_signatures, required_signers, aliased_addresses, epoch).is_None())
            ==> result.is_Err(),
        // S2: greedy succeeds (user tx) → Ok with greedy assignment
        (intent_is_sui_tx
            && !is_system_tx
            && !spec_any_addr_derivation_fails(tx_signatures)
            && tx_signatures@.len() == required_signers@.len()
            && spec_greedy_result(tx_signatures, required_signers, aliased_addresses, epoch).is_Some())
            ==> {
            &&& result.is_Ok()
            &&& ok_indices(&result) =~= spec_greedy_result(
                    tx_signatures, required_signers, aliased_addresses, epoch).get_Some_0()
        },
{
    unimplemented!()
}

} // verus!
