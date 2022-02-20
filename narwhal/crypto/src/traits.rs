// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use base64ct::Encoding;
use eyre::eyre;
use rand::{CryptoRng, RngCore};

use serde::{de::DeserializeOwned, Serialize};
pub use signature::{Error, Signer};
use std::fmt::Debug;

/// Trait impl'd by concrete types that represent digital cryptographic material
/// (keys). For signatures, we rely on `signature::Signature`, which may be more widely implemented.
///
/// Key types *must* (as mandated by the `AsRef<[u8]>` bound) be a thin
/// wrapper around the "bag-of-bytes" serialized form of a key which can
/// be directly parsed from or written to the "wire".
///
/// The [`ToFromBytes`] trait aims to provide similar simplicity by minimizing
/// the number of steps involved to obtain a serializable key and
/// ideally ensuring there is one signature type for any given signature system
/// shared by all "provider" crates.
///
/// For signature systems which require a more advanced internal representation
/// (e.g. involving decoded scalars or decompressed elliptic curve points) it's
/// recommended that "provider" libraries maintain their own internal signature
/// type and use `From` bounds to provide automatic conversions.
///
// This is essentially a copy of signature::Signature:
// - we can't implement signature::Signature on Pubkeys / PrivKeys w/o violating the orphan rule,
// - and we need a trait to base the definition of EncodeDecodeBase64 as an extension trait on.
pub trait ToFromBytes: AsRef<[u8]> + Debug + Sized {
    /// Parse a key from its byte representation
    fn from_bytes(bytes: &[u8]) -> Result<Self, Error>;

    /// Borrow a byte slice representing the serialized form of this key
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

/// Cryptographic material with an immediate conversion to/from Base64 strings.
///
/// This is an [extension trait](https://rust-lang.github.io/rfcs/0445-extension-trait-conventions.html) of `ToFromBytes` above.
///
pub trait EncodeDecodeBase64: Sized {
    fn encode_base64(&self) -> String;
    fn decode_base64(value: &str) -> Result<Self, eyre::Report>;
}

// The Base64ct is not strictly necessary for (PubKey|Signature), but this simplifies things a lot
impl<T: ToFromBytes> EncodeDecodeBase64 for T {
    fn encode_base64(&self) -> String {
        base64ct::Base64::encode_string(self.as_bytes())
    }

    fn decode_base64(value: &str) -> Result<Self, eyre::Report> {
        let bytes = base64ct::Base64::decode_vec(value).map_err(|e| eyre!("{}", e.to_string()))?;
        <T as ToFromBytes>::from_bytes(&bytes).map_err(|e| e.into())
    }
}

/// Trait impl'd by public keys in asymmetric cryptography.
///
/// The trait bounds are implemented so as to be symmetric and equivalent
/// to the ones on its associated types for private and signature material.
///
pub trait VerifyingKey:
    Serialize
    + DeserializeOwned
    + std::hash::Hash
    + Eq  // required to make some cached bytes representations explicit
    + Ord // required to put keys in BTreeMap
    + Default // see [#34](https://github.com/MystenLabs/narwhal/issues/34)
    + ToFromBytes
    + signature::Verifier<Self::Sig>
    + Send
    + Sync
    + 'static
    + Clone
{
    type PrivKey: SigningKey<PubKey = Self>;
    type Sig: Authenticator<PubKey = Self>;

    // Expected to be overridden by implementations
    fn verify_batch(msg: &[u8], pks: &[Self], sigs: &[Self::Sig]) -> Result<(), signature::Error> {
        if pks.len() != sigs.len() {
            return Err(signature::Error::new());
        }
        pks.iter()
            .zip(sigs)
            .try_for_each(|(pk, sig)| pk.verify(msg, sig))
    }
}

/// Trait impl'd by private (secret) keys in asymmetric cryptography.
///
/// The trait bounds are implemented so as to be symmetric and equivalent
/// to the ones on its associated types for public key and signature material.
///
pub trait SigningKey: ToFromBytes + Serialize + DeserializeOwned + Send + Sync + 'static {
    type PubKey: VerifyingKey<PrivKey = Self>;
    type Sig: Authenticator<PrivKey = Self>;
}

/// Trait impl'd by signatures in asymmetric cryptography.
///
/// The trait bounds are implemented so as to be symmetric and equivalent
/// to the ones on its associated types for private key and public key material.
///
pub trait Authenticator:
    signature::Signature + Default + Serialize + DeserializeOwned + Send + Sync + 'static + Clone
{
    type PubKey: VerifyingKey<Sig = Self>;
    type PrivKey: SigningKey<Sig = Self>;
}

/// Trait impl'd by a public / private key pair in asymmetric cryptography.
///
pub trait KeyPair {
    type PubKey: VerifyingKey<PrivKey = Self::PrivKey>;
    type PrivKey: SigningKey<PubKey = Self::PubKey>;
    fn public(&'_ self) -> &'_ Self::PubKey;
    fn private(self) -> Self::PrivKey;
    fn generate<R: CryptoRng + RngCore>(rng: &mut R) -> Self;
}
