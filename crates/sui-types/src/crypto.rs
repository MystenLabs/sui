// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::error::{SuiError, SuiResult};
use digest::Digest;
use ed25519_dalek as dalek;
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;
use std::{collections::{HashMap}, hash::Hash};
use crate::ed25519::{Ed25519KeyPair, Ed25519AuthoritySignature, Ed25519Signature, Ed25519PublicKeyBytes, Ed25519AuthorityQuorumSignInfo, Ed25519AuthoritySignInfo};
pub use crate::crypto_traits::{BcsSignable, Signable, SignableBytes};

pub type KeyPair = Ed25519KeyPair;
pub type Signature = Ed25519Signature;

// Change these to change signatures that Authorities use
pub type AuthoritySignature = Ed25519AuthoritySignature;
pub type PublicKeyBytes = Ed25519PublicKeyBytes;
pub type AuthorityQuorumSignInfo<const S: bool> = Ed25519AuthorityQuorumSignInfo<S>;
pub type AuthoritySignInfo = Ed25519AuthoritySignInfo;

/// AuthoritySignInfoTrait is a trait used specifically for a few structs in messages.rs
/// to template on whether the struct is signed by an authority. We want to limit how
/// those structs can be instanted on, hence the sealed trait.
/// TODO: We could also add the aggregated signature as another impl of the trait.
///       This will make CertifiedTransaction also an instance of the same struct.
pub trait AuthoritySignInfoTrait: private::SealedAuthoritySignInfoTrait {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptySignInfo {}
impl AuthoritySignInfoTrait for EmptySignInfo {}

impl AuthoritySignInfoTrait for AuthoritySignInfo {}

pub type AuthorityStrongQuorumSignInfo = AuthorityQuorumSignInfo<true>;
pub type AuthorityWeakQuorumSignInfo = AuthorityQuorumSignInfo<false>;

// Note: if you meet an error due to this line it may be because you need an Eq implementation for `CertifiedTransaction`,
// or one of the structs that include it, i.e. `ConfirmationTransaction`, `TransactionInfoResponse` or `ObjectInfoResponse`.
//
// Please note that any such implementation must be agnostic to the exact set of signatures in the certificate, as
// clients are allowed to equivocate on the exact nature of valid certificates they send to the system. This assertion
// is a simple tool to make sure certificates are accounted for correctly - should you remove it, you're on your own to
// maintain the invariant that valid certificates with distinct signatures are equivalent, but yet-unchecked
// certificates that differ on signers aren't.
//
// see also https://github.com/MystenLabs/sui/issues/266
static_assertions::assert_not_impl_any!(AuthorityStrongQuorumSignInfo: Hash, Eq, PartialEq);
static_assertions::assert_not_impl_any!(AuthorityWeakQuorumSignInfo: Hash, Eq, PartialEq);

impl<const S: bool> AuthoritySignInfoTrait for AuthorityQuorumSignInfo<S> {}


mod private {
    pub trait SealedAuthoritySignInfoTrait {}
    impl SealedAuthoritySignInfoTrait for super::EmptySignInfo {}
    impl SealedAuthoritySignInfoTrait for super::AuthoritySignInfo {}
    impl<const S: bool> SealedAuthoritySignInfoTrait for super::AuthorityQuorumSignInfo<S> {}
}

pub type PubKeyLookup = HashMap<PublicKeyBytes, dalek::PublicKey>;

#[derive(Default)]
pub struct VerificationObligation {
    lookup: PubKeyLookup,
    messages: Vec<Vec<u8>>,
    pub message_index: Vec<usize>,
    pub signatures: Vec<dalek::Signature>,
    pub public_keys: Vec<dalek::PublicKey>,
}

impl VerificationObligation {
    pub fn new(lookup: PubKeyLookup) -> VerificationObligation {
        VerificationObligation {
            lookup,
            ..Default::default()
        }
    }

    pub fn lookup_public_key(
        &mut self,
        key_bytes: &PublicKeyBytes,
    ) -> Result<dalek::PublicKey, SuiError> {
        match self.lookup.get(key_bytes) {
            Some(v) => Ok(*v),
            None => {
                let public_key = (*key_bytes).try_into()?;
                self.lookup.insert(*key_bytes, public_key);
                Ok(public_key)
            }
        }
    }

    /// Add a new message to the list of messages to be verified.
    /// Returns the index of the message.
    pub fn add_message(&mut self, message: Vec<u8>) -> usize {
        let idx = self.messages.len();
        self.messages.push(message);
        idx
    }

    pub fn verify_all(self) -> SuiResult<PubKeyLookup> {
        let messages_inner: Vec<_> = self
            .message_index
            .iter()
            .map(|idx| &self.messages[*idx][..])
            .collect();
        dalek::verify_batch(
            &messages_inner[..],
            &self.signatures[..],
            &self.public_keys[..],
        )
        .map_err(|error| SuiError::InvalidSignature {
            error: format!("{error}"),
        })?;

        Ok(self.lookup)
    }
}

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}
