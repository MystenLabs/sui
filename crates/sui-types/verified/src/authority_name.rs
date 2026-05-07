// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Definition of `AuthorityPublicKeyBytes` and its Verus specifications.
//!
//! The type lives here so that `impl View for AuthorityPublicKeyBytes` is
//! orphan-free: `sui-types-verified` owns the type. `sui-types` depends on
//! this crate and re-exports the type, so all existing imports are unaffected.
//!
//! `sui-types-verified` does NOT depend on `sui-types` — that would create a
//! cycle. Shared helpers (like `Readable`) are copied into `serde_helpers`.

use crate::serde_helpers::Readable;
use anyhow::{Error, anyhow};
use derive_more::AsRef;
use fastcrypto::bls12381::min_sig::BLS12381PublicKey;
use fastcrypto::encoding::{Base64, Encoding, Hex};
use fastcrypto::error::FastCryptoError;
use fastcrypto::traits::ToFromBytes;
use fastcrypto::traits::VerifyingKey;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use sui_sdk_types::Bls12381PublicKey;
use vstd::prelude::*;
#[cfg(verus_only)]
use vstd::std_specs::hash::obeys_key_model;

/// Compressed representation of an authority's public key that we pass
/// around in Sui. Defined here (in the verified crate) so that Verus can
/// attach a `View` impl without orphan-rule issues.
///
/// `sui-types` re-exports this type, so all existing `sui_types::crypto::AuthorityPublicKeyBytes`
/// imports continue to work unchanged.
#[serde_as]
#[derive(
    Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, AsRef,
)]
#[as_ref(forward)]
pub struct AuthorityPublicKeyBytes(
    #[schemars(with = "Base64")]
    #[serde_as(as = "Readable<Base64, Bytes>")]
    pub [u8; BLS12381PublicKey::LENGTH],
);

impl AuthorityPublicKeyBytes {
    fn fmt_impl(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = Hex::encode(self.0);
        write!(f, "k#{}", s)?;
        Ok(())
    }

    pub const ZERO: Self = Self::new([0u8; BLS12381PublicKey::LENGTH]);

    /// This ensures it's impossible to construct an instance with other than registered lengths.
    pub const fn new(bytes: [u8; BLS12381PublicKey::LENGTH]) -> AuthorityPublicKeyBytes {
        AuthorityPublicKeyBytes(bytes)
    }
}

impl Debug for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl Display for AuthorityPublicKeyBytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        self.fmt_impl(f)
    }
}

impl ToFromBytes for AuthorityPublicKeyBytes {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        let bytes: [u8; BLS12381PublicKey::LENGTH] = bytes
            .try_into()
            .map_err(|_| FastCryptoError::InvalidInput)?;
        Ok(AuthorityPublicKeyBytes(bytes))
    }
}

// `AuthorityPublicKey` in sui-types is `type AuthorityPublicKey = BLS12381PublicKey`,
// so this impl is identical to `From<&AuthorityPublicKey>`.
impl From<&BLS12381PublicKey> for AuthorityPublicKeyBytes {
    fn from(pk: &BLS12381PublicKey) -> AuthorityPublicKeyBytes {
        AuthorityPublicKeyBytes::from_bytes(pk.as_ref()).unwrap()
    }
}

impl FromStr for AuthorityPublicKeyBytes {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = Hex::decode(s).map_err(|e| anyhow!(e))?;
        Self::from_bytes(&value[..]).map_err(|e| anyhow!(e))
    }
}

impl Default for AuthorityPublicKeyBytes {
    fn default() -> Self {
        Self::ZERO
    }
}

// `AuthorityPublicKey = BLS12381PublicKey` in sui-types. This impl must live
// here because both types are foreign to sui-types after the move.
impl TryFrom<AuthorityPublicKeyBytes> for BLS12381PublicKey {
    type Error = FastCryptoError;

    fn try_from(bytes: AuthorityPublicKeyBytes) -> Result<BLS12381PublicKey, Self::Error> {
        BLS12381PublicKey::from_bytes(bytes.as_ref())
    }
}

// Conversions between AuthorityPublicKeyBytes and sdk types. These live here
// because sui-types-verified owns AuthorityPublicKeyBytes; they cannot stay in
// sui-types after the move (both sides would be foreign there).

impl From<AuthorityPublicKeyBytes> for Bls12381PublicKey {
    fn from(value: AuthorityPublicKeyBytes) -> Self {
        Self::new(value.0)
    }
}

impl From<Bls12381PublicKey> for AuthorityPublicKeyBytes {
    fn from(value: Bls12381PublicKey) -> Self {
        Self::new(value.into_inner())
    }
}

// ---------------------------------------------------------------------------
// Verus specifications
// ---------------------------------------------------------------------------
//
// `AuthorityPublicKeyBytes` is defined here, so that we can write `impl View for
// AuthorityPublicKeyBytes` without having an orphan issue.

verus! {

// Never referenced directly. The `external_type_specification` attribute is
// what matters: it tells Verus that `AuthorityPublicKeyBytes` (defined outside
// `verus! { }`, because its serde derives aren't Verus-aware) may appear in
// spec contexts and have spec traits like `View` implemented for it.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExAuthorityPublicKeyBytes(pub AuthorityPublicKeyBytes);

// Identity view: `k@ == k` for all `k`.
// This makes `Map<AuthorityName, V>` the spec view type for any
// `HashMapWithView<AuthorityName, V>`, enabling full vstd specs everywhere.
impl View for AuthorityPublicKeyBytes {
    type V = AuthorityPublicKeyBytes;
    open spec fn view(&self) -> AuthorityPublicKeyBytes { *self }
}

/// Axiom: `AuthorityPublicKeyBytes` has consistent `Hash` and `Eq` impls.
///
/// Both are derived from the inner `[u8; LENGTH]` array, satisfying the
/// algebraic contract `obeys_key_model` requires. With this broadcast lemma
/// in scope, any `HashMapWithView<AuthorityName, V>` gets full vstd specs.
#[verifier::external_body]
pub broadcast proof fn axiom_authority_name_key_model()
    ensures #[trigger] obeys_key_model::<AuthorityPublicKeyBytes>(),
{}

} // verus!
