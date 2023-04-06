// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use fastcrypto::{
    bls12381, ed25519,
    error::FastCryptoError,
    hash::{Blake2b256, HashFunction},
    traits::{AggregateAuthenticator, Signer, VerifyingKey},
};

// This re-export allows using the trait-defined APIs
pub use fastcrypto::traits;
use serde::Serialize;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope, INTENT_PREFIX_LENGTH};

////////////////////////////////////////////////////////////////////////
/// Type aliases selecting the signature algorithm for the code base.
////////////////////////////////////////////////////////////////////////
// Here we select the types that are used by default in the code base.
// The whole code base should only:
// - refer to those aliases and not use the individual scheme implementations
// - not use the schemes in a way that break genericity (e.g. using their Struct impl functions)
// - swap one of those aliases to point to another type if necessary
//
// Beware: if you change those aliases to point to another scheme implementation, you will have
// to change all four aliases to point to concrete types that work with each other. Failure to do
// so will result in a ton of compilation errors, and worse: it will not make sense!

pub type PublicKey = bls12381::min_sig::BLS12381PublicKey;
pub type PublicKeyBytes = bls12381::min_sig::BLS12381PublicKeyAsBytes;
pub type Signature = bls12381::min_sig::BLS12381Signature;
pub type AggregateSignature = bls12381::min_sig::BLS12381AggregateSignature;
pub type AggregateSignatureBytes = bls12381::min_sig::BLS12381AggregateSignatureAsBytes;
pub type PrivateKey = bls12381::min_sig::BLS12381PrivateKey;
pub type KeyPair = bls12381::min_sig::BLS12381KeyPair;

pub type NetworkPublicKey = ed25519::Ed25519PublicKey;
pub type NetworkKeyPair = ed25519::Ed25519KeyPair;

////////////////////////////////////////////////////////////////////////

// Type alias selecting the default hash function for the code base.
pub type DefaultHashFunction = Blake2b256;
pub const DIGEST_LENGTH: usize = DefaultHashFunction::OUTPUT_SIZE;
pub const INTENT_MESSAGE_LENGTH: usize = INTENT_PREFIX_LENGTH + DIGEST_LENGTH;

/// A trait for sign and verify over an intent message, instead of the message itself. See more at [struct IntentMessage].
pub trait NarwhalAuthoritySignature {
    /// Create a new signature over an intent message.
    fn new_secure<T>(value: &IntentMessage<T>, secret: &dyn Signer<Self>) -> Self
    where
        T: Serialize;

    /// Verify the signature on an intent message against the public key.
    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        author: &PublicKey,
    ) -> Result<(), FastCryptoError>
    where
        T: Serialize;
}

impl NarwhalAuthoritySignature for Signature {
    fn new_secure<T>(value: &IntentMessage<T>, secret: &dyn Signer<Self>) -> Self
    where
        T: Serialize,
    {
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        secret.sign(&message)
    }

    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        public_key: &PublicKey,
    ) -> Result<(), FastCryptoError>
    where
        T: Serialize,
    {
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        public_key.verify(&message, self)
    }
}

pub trait NarwhalAuthorityAggregateSignature {
    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        pks: &[PublicKey],
    ) -> Result<(), FastCryptoError>
    where
        T: Serialize;
}

impl NarwhalAuthorityAggregateSignature for AggregateSignature {
    fn verify_secure<T>(
        &self,
        value: &IntentMessage<T>,
        pks: &[PublicKey],
    ) -> Result<(), FastCryptoError>
    where
        T: Serialize,
    {
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        self.verify(pks, &message)
    }
}

/// Wrap a message in an intent message. Currently in Narwhal, the scope is always IntentScope::HeaderDigest and the app id is AppId::Narwhal.
pub fn to_intent_message<T>(value: T) -> IntentMessage<T> {
    IntentMessage::new(Intent::narwhal_app(IntentScope::HeaderDigest), value)
}
