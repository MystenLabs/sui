// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Enum, SimpleObject, Union};
use sui_types::crypto::{CompressedSignature, PublicKey};
use sui_types::multisig::MultiSig;

use crate::api::scalars::base64::Base64;

/// An aggregated multisig signature.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigSignature {
    /// The individual member signatures, one per signer who participated.
    signatures: Vec<MultisigMemberSignature>,
    /// A bitmap indicating which members of the committee signed.
    bitmap: u16,
    /// The multisig committee (public keys + weights + threshold).
    committee: MultisigCommittee,
}

/// A single member's signature within a multisig.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigMemberSignature {
    /// The signature scheme used by this member.
    scheme: MultisigMemberSignatureScheme,
    /// The raw signature bytes (without public key).
    signature: Base64,
}

/// The signature scheme of a multisig member's signature.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MultisigMemberSignatureScheme {
    #[graphql(name = "ED25519")]
    Ed25519,
    #[graphql(name = "SECP256K1")]
    Secp256k1,
    #[graphql(name = "SECP256R1")]
    Secp256r1,
    #[graphql(name = "ZKLOGIN")]
    ZkLogin,
    #[graphql(name = "PASSKEY")]
    Passkey,
}

/// The multisig committee definition.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigCommittee {
    /// The committee members (public key + weight).
    members: Vec<MultisigMember>,
    /// The threshold number of weight needed for a valid multisig.
    threshold: u16,
}

/// A single member of a multisig committee.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigMember {
    /// The member's public key.
    public_key: MultisigMemberPublicKey,
    /// The member's weight in the committee.
    weight: u8,
}

/// A multisig member's public key, varying by scheme.
#[derive(Union, Clone)]
pub(crate) enum MultisigMemberPublicKey {
    Ed25519(Ed25519PublicKey),
    Secp256k1(Secp256k1PublicKey),
    Secp256r1(Secp256r1PublicKey),
    Passkey(PasskeyPublicKey),
    ZkLogin(ZkLoginPublicIdentifier),
}

/// An Ed25519 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Ed25519PublicKey {
    /// The raw public key bytes.
    bytes: Base64,
}

/// A Secp256k1 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256k1PublicKey {
    /// The raw public key bytes.
    bytes: Base64,
}

/// A Secp256r1 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256r1PublicKey {
    /// The raw public key bytes.
    bytes: Base64,
}

/// A Passkey public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct PasskeyPublicKey {
    /// The raw public key bytes.
    bytes: Base64,
}

/// A zkLogin public identifier, containing the OAuth issuer and address seed.
#[derive(SimpleObject, Clone, Default)]
pub(crate) struct ZkLoginPublicIdentifier {
    /// The OAuth provider issuer string (e.g. "https://accounts.google.com").
    iss: String,
    /// The address seed as a decimal string.
    address_seed: String,
}

impl From<&MultiSig> for MultisigSignature {
    fn from(m: &MultiSig) -> Self {
        Self {
            signatures: m
                .get_sigs()
                .iter()
                .map(MultisigMemberSignature::from)
                .collect(),
            bitmap: m.get_bitmap(),
            committee: MultisigCommittee::from(m.get_pk()),
        }
    }
}

impl From<&CompressedSignature> for MultisigMemberSignature {
    fn from(sig: &CompressedSignature) -> Self {
        let (scheme, bytes): (_, &[u8]) = match sig {
            CompressedSignature::Ed25519(b) => (MultisigMemberSignatureScheme::Ed25519, &b.0),
            CompressedSignature::Secp256k1(b) => (MultisigMemberSignatureScheme::Secp256k1, &b.0),
            CompressedSignature::Secp256r1(b) => (MultisigMemberSignatureScheme::Secp256r1, &b.0),
            CompressedSignature::ZkLogin(b) => {
                (MultisigMemberSignatureScheme::ZkLogin, b.0.as_slice())
            }
            CompressedSignature::Passkey(b) => {
                (MultisigMemberSignatureScheme::Passkey, b.0.as_slice())
            }
        };
        Self {
            scheme,
            signature: Base64(bytes.to_vec()),
        }
    }
}

impl From<&sui_types::multisig::MultiSigPublicKey> for MultisigCommittee {
    fn from(pk: &sui_types::multisig::MultiSigPublicKey) -> Self {
        Self {
            members: pk
                .pubkeys()
                .iter()
                .map(|(public_key, weight)| MultisigMember {
                    public_key: MultisigMemberPublicKey::from(public_key),
                    weight: *weight,
                })
                .collect(),
            threshold: *pk.threshold(),
        }
    }
}

impl From<&PublicKey> for MultisigMemberPublicKey {
    fn from(pk: &PublicKey) -> Self {
        match pk {
            PublicKey::Ed25519(_) => MultisigMemberPublicKey::Ed25519(Ed25519PublicKey {
                bytes: Base64(pk.as_ref().to_vec()),
            }),
            PublicKey::Secp256k1(_) => MultisigMemberPublicKey::Secp256k1(Secp256k1PublicKey {
                bytes: Base64(pk.as_ref().to_vec()),
            }),
            PublicKey::Secp256r1(_) => MultisigMemberPublicKey::Secp256r1(Secp256r1PublicKey {
                bytes: Base64(pk.as_ref().to_vec()),
            }),
            PublicKey::Passkey(_) => MultisigMemberPublicKey::Passkey(PasskeyPublicKey {
                bytes: Base64(pk.as_ref().to_vec()),
            }),
            PublicKey::ZkLogin(z) => {
                // Convert through sui_sdk_types for clean field extraction.
                MultisigMemberPublicKey::ZkLogin(
                    sui_sdk_types::ZkLoginPublicIdentifier::try_from(z.to_owned())
                        .map(|id| ZkLoginPublicIdentifier {
                            iss: id.iss().to_owned(),
                            address_seed: id.address_seed().to_string(),
                        })
                        .unwrap_or_default(),
                )
            }
        }
    }
}
