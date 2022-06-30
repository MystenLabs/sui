// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{error::{SuiError, SuiResult}};
use digest::Digest;
use ed25519_dalek as dalek;
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;
use std::{collections::{HashMap}, hash::Hash};
pub use crate::crypto_traits::{BcsSignable, Signable, SignableBytes};

// Change these to edDSA
use crate::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519AuthoritySignature, Ed25519Signature, Ed25519PublicKeyBytes, Ed25519AuthorityQuorumSignInfo, Ed25519AuthoritySignInfo};
pub type KeyPair = Ed25519KeyPair;
pub type PublicKey = Ed25519PublicKey;
pub type Signature = Ed25519Signature;
pub type AuthoritySignature = Ed25519AuthoritySignature;
pub type PublicKeyBytes = Ed25519PublicKeyBytes;
pub type AuthorityQuorumSignInfo<const S: bool> = Ed25519AuthorityQuorumSignInfo<S>;
pub type AuthoritySignInfo = Ed25519AuthoritySignInfo;


// UNCOMMENT TO CHANGE SIGNATURE SCHEME TO BLS

// use crate::bls12381::{Bls12381PublicKey, Bls12381KeyPair, Bls12381Signature, Bls12381AuthoritySignature, Bls12381PublicKeyBytes, Bls12381AuthorityQuorumSignInfo, Bls12381AuthoritySignInfo};
// pub type KeyPair = Bls12381KeyPair;
// pub type PublicKey = Bls12381PublicKey;
// pub type Signature = Bls12381Signature;
// pub type AuthoritySignature = Bls12381AuthoritySignature;
// pub type PublicKeyBytes = Bls12381PublicKeyBytes;
// pub type AuthorityQuorumSignInfo<const S: bool> = Bls12381AuthorityQuorumSignInfo<S>;
// pub type AuthoritySignInfo = Bls12381AuthoritySignInfo;

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

pub fn sha3_hash<S: Signable<Sha3_256>>(signable: &S) -> [u8; 32] {
    let mut digest = Sha3_256::default();
    signable.write(&mut digest);
    let hash = digest.finalize();
    hash.into()
}
