// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A secret seed value, useful for deterministic private key and SuiAddress generation.

use fastcrypto::{hkdf::hkdf_generate_from_ikm, traits::KeyPair as KeypairTraits};
use rand::{CryptoRng, RngCore};
use sha3::Sha3_256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::base_types::SuiAddress;
use crate::crypto::{Signable, Signature, SuiPublicKey};
use crate::error::SuiError;

#[cfg(test)]
#[path = "unit_tests/signature_seed_tests.rs"]
mod signature_seed_tests;

/// The length of a `secret crypto seed`, in bytes.
pub const SEED_LENGTH: usize = 32;

/// A secret seed required for various cryptographic purposes, i.e., deterministic key derivation.
///
/// Instances of this seed are automatically overwritten with zeroes when they
/// fall out of scope.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SignatureSeed([u8; SEED_LENGTH]);

/// Return the bytes of this seed.
impl AsRef<[u8]> for SignatureSeed {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl SignatureSeed {
    /// Convert this seed value to a byte array.
    #[inline]
    pub fn to_bytes(&self) -> [u8; SEED_LENGTH] {
        self.0
    }

    /// View this seed as a byte array.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; SEED_LENGTH] {
        &self.0
    }

    /// Construct a `Seed` from a slice of bytes.
    ///
    /// # Example
    ///
    /// ```
    /// use sui_types::signature_seed::SignatureSeed;
    /// use sui_types::error::SuiError;
    /// use sui_types::signature_seed::SEED_LENGTH;
    /// # fn doctest() -> Result<SignatureSeed, SuiError> {
    /// let secret_bytes: [u8; SEED_LENGTH] = [
    ///    112, 012, 187, 211, 011, 092, 030, 001,
    ///    225, 255, 000, 166, 112, 236, 044, 196,
    ///    068, 073, 197, 105, 123, 050, 105, 025,
    ///    112, 059, 172, 003, 028, 174, 127, 096, ];
    ///
    /// let seed: SignatureSeed = SignatureSeed::from_bytes(&secret_bytes)?;
    /// Ok(seed)
    /// # }
    /// #
    /// # fn main() {
    /// #     let result = doctest();
    /// #     assert!(result.is_ok());
    /// # }
    /// ```
    ///
    /// # Input
    ///
    /// A byte array that represents the secret bytes of this `SignatureSeed`.
    ///
    /// # Returns
    ///
    /// A `Result` whose okay value is a `SignatureSeed` or whose error value
    /// is a `SuiError` wrapping the internal error that occurred.
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<SignatureSeed, SuiError> {
        if bytes.len() != SEED_LENGTH {
            return Err(SuiError::SignatureSeedInvalidLength(bytes.len()));
        }
        let mut bits: [u8; SEED_LENGTH] = [0u8; SEED_LENGTH];
        bits.copy_from_slice(&bytes[..SEED_LENGTH]);

        Ok(SignatureSeed(bits))
    }

    /// Generate a `SignatureSeed` from a `csprng`.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate rand;
    /// use rand::rngs::OsRng;
    /// use sui_types::signature_seed::SignatureSeed;
    /// # fn main() {
    ///     let mut csprng = OsRng{};
    ///     let secret_key: SignatureSeed = SignatureSeed::generate(&mut csprng);
    /// # }
    /// ```
    ///
    /// # Input
    ///
    /// A CSPRNG with a `fill_bytes()` method, e.g. `rand::OsRng`
    ///
    /// # Returns
    ///
    /// A fresh random `SignatureSeed`.
    pub fn generate<T>(csprng: &mut T) -> SignatureSeed
    where
        T: CryptoRng + RngCore,
    {
        let mut sk: SignatureSeed = SignatureSeed([0u8; SEED_LENGTH]);
        csprng.fill_bytes(&mut sk.0);
        sk
    }

    /// Deterministically generate a SuiAddress via HKDF.
    ///
    /// # Example
    ///
    /// ```
    /// use serde::{Deserialize, Serialize};
    /// use sui_types::signature_seed::SignatureSeed;
    /// use sui_types::crypto::AccountKeyPair;
    ///
    /// # fn main() {
    ///     // In production this SHOULD be a secret seed value, here we pin it for demo purposes.
    ///     let seed = SignatureSeed::from_bytes(&[5u8; 32]).unwrap();
    ///
    ///     // An input id.
    ///     let id = "some-user@some-domain.com".as_bytes();
    ///
    ///     // Some domain.
    ///     let domain = "some-application".as_bytes();
    ///
    ///     // Get address for the provided `id`.
    ///     let sui_address = seed
    ///         .new_deterministic_address::<AccountKeyPair>(&id, Some(&domain))
    ///         .unwrap();
    /// # }
    /// ```
    ///
    /// # Input
    ///
    /// A user `id` byte-array, i.e., a username or email address.
    /// A `domain` separation value (optional), to distinguish between purposes of key derivation.
    ///
    /// # Returns
    ///
    /// A derived `SuiAddress`, generated deterministically from some `seed`, `id` and `domain`.
    pub fn new_deterministic_address<K: KeypairTraits>(
        &self,
        id: &[u8],
        domain: Option<&[u8]>,
    ) -> Result<SuiAddress, SuiError>
    where
        <K as KeypairTraits>::PubKey: SuiPublicKey,
    {
        let keypair = SignatureSeed::new_deterministic_keypair::<K>(self, id, domain)?;
        Ok(keypair.public().into())
    }

    /// Sign a message using a deterministically derived key from some `id` input.
    ///
    /// # Example
    ///
    /// ```
    /// use serde::{Deserialize, Serialize};
    /// use sui_types::signature_seed::SignatureSeed;
    /// use sui_types::crypto::{SuiSignature, AccountKeyPair};
    /// use sui_types::crypto::bcs_signable_test::Foo;
    ///
    /// // The BcsSignable trait is implemented as a sealed trait, so this is equivalent to the
    /// // following, with the BcsSignable impl. mandatorily situated in the bcs_signable module:
    /// // #[derive(Serialize, Deserialize)]
    /// // struct Foo(String);
    /// // impl BcsSignable for Foo {}
    ///
    /// # fn main() {
    ///     // In production this SHOULD be a secret seed value, here we pin it for demo purposes.
    ///     let seed = SignatureSeed::from_bytes(&[5u8; 32]).unwrap();
    ///
    ///     // An input id.
    ///     let id = "some-user@some-domain.com".as_bytes();
    ///
    ///     // Some domain.
    ///     let domain = "some-application".as_bytes();
    ///
    ///     // The msg to sign (note that we can only sign `Signable` objects.
    ///     let msg = Foo("some-signable-message".to_string());
    ///
    ///     let signature = seed.sign::<_, AccountKeyPair>(&id, Some(&domain), &msg).unwrap();
    ///
    ///     // Get address for the provided `id`.
    ///     let sui_address = seed
    ///         .new_deterministic_address::<AccountKeyPair>(&id, Some(&domain))
    ///         .unwrap();
    ///     let verification = signature.verify(&msg, sui_address);
    ///     assert!(verification.is_ok());
    /// # }
    /// ```
    ///
    /// # Input
    ///
    /// A user `id` byte-array, i.e., a username or email address.
    /// A `domain` separation value (optional), to distinguish between purposes of key derivation.
    /// A Signable `value` (the message to be signed).
    ///
    /// # Returns
    ///
    /// A `Result` whose okay value is a `Signature` or whose error value
    /// is a `signature::Error` wrapping the internal error that occurred.
    pub fn sign<T, K: KeypairTraits + signature::Signer<Signature>>(
        &self,
        id: &[u8],
        domain: Option<&[u8]>,
        value: &T,
    ) -> Result<Signature, signature::Error>
    where
        T: Signable<Vec<u8>>,
    {
        let keypair: K = SignatureSeed::new_deterministic_keypair(self, id, domain)
            .map_err(|_| signature::Error::new())?;
        Ok(Signature::new_legacy(value, &keypair))
    }

    // Deterministically generate an ed25519 public key via HKDF.
    pub fn new_deterministic_keypair<K: KeypairTraits>(
        &self,
        id: &[u8],
        domain: Option<&[u8]>,
    ) -> Result<K, SuiError> {
        hkdf_generate_from_ikm::<Sha3_256, K>(&self.0, id, domain)
            .map_err(|_| SuiError::HkdfError("Deterministic keypair derivation failed".to_string()))
    }
}

/// An all zeros seed.
impl Default for SignatureSeed {
    fn default() -> Self {
        SignatureSeed([0u8; SEED_LENGTH])
    }
}
