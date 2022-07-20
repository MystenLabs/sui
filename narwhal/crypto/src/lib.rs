// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use base64ct::{Base64, Encoding};
use blake2::{digest::VariableOutput, VarBlake2b};

use rand::{rngs::OsRng, CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

pub use signature::{Signature, Verifier};
use std::fmt;
use tokio::sync::{
    mpsc::{channel, Sender},
    oneshot,
};
use traits::{Authenticator, KeyPair};

#[cfg(test)]
#[path = "tests/pubkey_bytes_tests.rs"]
pub mod pubkey_bytes_tests;

#[cfg(test)]
#[path = "tests/ed25519_tests.rs"]
pub mod ed25519_tests;

#[cfg(test)]
#[path = "tests/secp256k1_tests.rs"]
pub mod secp256k1_tests;

#[cfg(all(test, feature = "celo"))]
#[path = "tests/bls12377_tests.rs"]
pub mod bls12377_tests;

#[cfg(feature = "celo")]
pub mod bls12377;
#[cfg(test)]
#[path = "tests/bls12381_tests.rs"]
pub mod bls12381_tests;

pub mod bls12381;
pub mod ed25519;
pub mod hkdf;
pub mod pubkey_bytes;
pub mod serde_helpers;

pub mod secp256k1;

pub mod traits;

pub type CryptoError = ed25519_dalek::ed25519::Error;

pub const DIGEST_LEN: usize = 32;

pub fn blake2b_256<F: Fn(&mut blake2::VarBlake2b)>(closure: F) -> [u8; DIGEST_LEN] {
    let mut hasher = VarBlake2b::new(DIGEST_LEN).unwrap();
    closure(&mut hasher);
    let mut res = [0u8; DIGEST_LEN];
    hasher.finalize_variable(|output| res.copy_from_slice(output));
    res
}

/// Represents a hash digest (32 bytes).
#[derive(Hash, PartialEq, Default, Eq, Clone, Deserialize, Serialize, Ord, PartialOrd, Copy)]
pub struct Digest([u8; DIGEST_LEN]);

impl Digest {
    pub fn new(val: [u8; DIGEST_LEN]) -> Self {
        Digest(val)
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub fn size(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(&self.0))
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", Base64::encode_string(&self.0).get(0..16).unwrap())
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// This trait is implemented by all messages that can be hashed.
pub trait Hash {
    type TypedDigest: Into<Digest> + std::fmt::Display + std::hash::Hash + Eq + Ord + Copy;
    fn digest(&self) -> Self::TypedDigest;
}

////////////////////////////////////////////////////////////////
// Generic Keypair
////////////////////////////////////////////////////////////////

pub fn generate_production_keypair<K: KeyPair>() -> K {
    generate_keypair::<K, _>(&mut OsRng)
}

pub fn generate_keypair<K: KeyPair, R>(csprng: &mut R) -> K
where
    R: CryptoRng + RngCore,
{
    K::generate(csprng)
}

/// This service holds the node's private key. It takes digests as input and returns a signature
/// over the digest (through a one-shot channel).
#[derive(Clone)]
pub struct SignatureService<Signature: Authenticator> {
    channel: Sender<(Digest, oneshot::Sender<Signature>)>,
}

impl<Signature: Authenticator> SignatureService<Signature> {
    pub fn new<S>(signer: S) -> Self
    where
        S: signature::Signer<Signature> + Send + 'static,
    {
        let (tx, mut rx): (Sender<(Digest, oneshot::Sender<_>)>, _) = channel(100);
        tokio::spawn(async move {
            while let Some((digest, sender)) = rx.recv().await {
                let signature = signer.sign(&digest.0);
                let _ = sender.send(signature);
            }
        });
        Self { channel: tx }
    }

    pub async fn request_signature(&mut self, digest: Digest) -> Signature {
        let (sender, receiver): (oneshot::Sender<_>, oneshot::Receiver<_>) = oneshot::channel();
        if let Err(e) = self.channel.send((digest, sender)).await {
            panic!("Failed to send message Signature Service: {e}");
        }
        receiver
            .await
            .expect("Failed to receive signature from Signature Service")
    }
}
