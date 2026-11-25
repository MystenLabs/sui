// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{SimpleObject, Union};
use fastcrypto::traits::ToFromBytes;
use sui_types::crypto::{CompressedSignature, PublicKey};
use sui_types::multisig::MultiSig;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use sui_types::zk_login_authenticator::ZkLoginAuthenticatorV2;

use crate::api::scalars::base64::Base64;
use crate::api::types::user_signature::passkey::PasskeySignature;
use crate::api::types::user_signature::zklogin::{ZkLoginSignature, ZkLoginV2Signature};
use crate::api::types::user_signature::{
    Ed25519Signature, MlDsa65Signature, Secp256k1Signature, Secp256r1Signature, SignatureScheme,
};

/// An aggregated multisig signature.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigSignature {
    /// The individual member signatures, one per signer who participated.
    /// Compressed signatures within a multisig do not include the signer's public key,
    /// so `publicKey` will be `null` for simple signature schemes (Ed25519, Secp256k1, Secp256r1).
    signatures: Option<Vec<SignatureScheme>>,
    /// A bitmap indicating which members of the committee signed.
    bitmap: Option<u16>,
    /// The multisig committee (public keys + weights + threshold).
    committee: Option<MultisigCommittee>,
}

/// The multisig committee definition.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigCommittee {
    /// The committee members (public key + weight).
    members: Option<Vec<MultisigMember>>,
    /// The threshold number of weight needed for a valid multisig.
    threshold: Option<u16>,
}

/// A single member of a multisig committee.
#[derive(SimpleObject, Clone)]
pub(crate) struct MultisigMember {
    /// The member's public key.
    public_key: Option<MultisigMemberPublicKey>,
    /// The member's weight in the committee.
    weight: Option<u8>,
}

/// A multisig member's public key, varying by scheme.
#[derive(Union, Clone)]
pub(crate) enum MultisigMemberPublicKey {
    Ed25519(Ed25519PublicKey),
    Secp256k1(Secp256k1PublicKey),
    Secp256r1(Secp256r1PublicKey),
    Passkey(PasskeyPublicKey),
    ZkLogin(ZkLoginPublicIdentifier),
    MlDsa65(MlDsa65PublicKey),
}

/// An Ed25519 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Ed25519PublicKey {
    /// The raw public key bytes.
    bytes: Option<Base64>,
}

/// An ML-DSA-65 (FIPS 204) public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct MlDsa65PublicKey {
    /// The raw public key bytes.
    bytes: Option<Base64>,
}

/// A Secp256k1 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256k1PublicKey {
    /// The raw public key bytes.
    bytes: Option<Base64>,
}

/// A Secp256r1 public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256r1PublicKey {
    /// The raw public key bytes.
    bytes: Option<Base64>,
}

/// A Passkey public key.
#[derive(SimpleObject, Clone)]
pub(crate) struct PasskeyPublicKey {
    /// The raw public key bytes.
    bytes: Option<Base64>,
}

/// A zkLogin public identifier, containing the OAuth issuer and address seed.
#[derive(SimpleObject, Clone, Default)]
pub(crate) struct ZkLoginPublicIdentifier {
    /// The OAuth provider issuer string (e.g. "https://accounts.google.com").
    pub(crate) iss: Option<String>,
    /// The address seed as a decimal string.
    pub(crate) address_seed: Option<String>,
}

impl ZkLoginPublicIdentifier {
    pub(super) fn new(iss: &str, address_seed: String) -> Self {
        Self {
            iss: Some(iss.to_owned()),
            address_seed: Some(address_seed),
        }
    }
}

impl From<&MultiSig> for MultisigSignature {
    fn from(m: &MultiSig) -> Self {
        Self {
            signatures: Some(
                m.get_sigs()
                    .iter()
                    .filter_map(compressed_signature_to_scheme)
                    .collect(),
            ),
            bitmap: Some(m.get_bitmap()),
            committee: Some(MultisigCommittee::from(m.get_pk())),
        }
    }
}

/// Converts a `CompressedSignature` into a `SignatureScheme`.
/// Compressed signatures within a multisig do not include the signer's public key,
/// so `public_key` will be `None` for simple signature schemes.
fn compressed_signature_to_scheme(sig: &CompressedSignature) -> Option<SignatureScheme> {
    match sig {
        CompressedSignature::Ed25519(b) => Some(SignatureScheme::Ed25519(Ed25519Signature {
            signature: Some(Base64(b.0.to_vec())),
            public_key: None,
        })),
        CompressedSignature::Secp256k1(b) => Some(SignatureScheme::Secp256k1(Secp256k1Signature {
            signature: Some(Base64(b.0.to_vec())),
            public_key: None,
        })),
        CompressedSignature::Secp256r1(b) => Some(SignatureScheme::Secp256r1(Secp256r1Signature {
            signature: Some(Base64(b.0.to_vec())),
            public_key: None,
        })),
        CompressedSignature::ZkLogin(b) => ZkLoginAuthenticator::from_bytes(&b.0)
            .ok()
            .map(|native| SignatureScheme::ZkLogin(ZkLoginSignature { native })),
        CompressedSignature::ZkLoginV2(b) => ZkLoginAuthenticatorV2::from_bytes(&b.0)
            .ok()
            .map(|native| SignatureScheme::ZkLoginV2(ZkLoginV2Signature { native })),
        CompressedSignature::Passkey(b) => {
            bcs::from_bytes::<sui_types::passkey_authenticator::PasskeyAuthenticator>(&b.0)
                .ok()
                .map(|native| SignatureScheme::Passkey(PasskeySignature { native }))
        }
        CompressedSignature::MLDSA65(b) => Some(SignatureScheme::MlDsa65(MlDsa65Signature {
            signature: Some(Base64(b.0.to_vec())),
            public_key: None,
        })),
    }
}

impl From<&sui_types::multisig::MultiSigPublicKey> for MultisigCommittee {
    fn from(pk: &sui_types::multisig::MultiSigPublicKey) -> Self {
        Self {
            members: Some(
                pk.pubkeys()
                    .iter()
                    .map(|(public_key, weight)| MultisigMember {
                        public_key: Some(MultisigMemberPublicKey::from(public_key)),
                        weight: Some(*weight),
                    })
                    .collect(),
            ),
            threshold: Some(*pk.threshold()),
        }
    }
}

impl From<&PublicKey> for MultisigMemberPublicKey {
    fn from(pk: &PublicKey) -> Self {
        match pk {
            PublicKey::Ed25519(_) => MultisigMemberPublicKey::Ed25519(Ed25519PublicKey {
                bytes: Some(Base64(pk.as_ref().to_vec())),
            }),
            PublicKey::Secp256k1(_) => MultisigMemberPublicKey::Secp256k1(Secp256k1PublicKey {
                bytes: Some(Base64(pk.as_ref().to_vec())),
            }),
            PublicKey::Secp256r1(_) => MultisigMemberPublicKey::Secp256r1(Secp256r1PublicKey {
                bytes: Some(Base64(pk.as_ref().to_vec())),
            }),
            PublicKey::Passkey(_) => MultisigMemberPublicKey::Passkey(PasskeyPublicKey {
                bytes: Some(Base64(pk.as_ref().to_vec())),
            }),
            PublicKey::ZkLogin(z) | PublicKey::ZkLoginV2(z) => MultisigMemberPublicKey::ZkLogin(
                sui_sdk_types::ZkLoginPublicIdentifier::try_from(z.to_owned())
                    .map(|id| ZkLoginPublicIdentifier::new(id.iss(), id.address_seed().to_string()))
                    .unwrap_or_default(),
            ),
            PublicKey::MLDSA65(_) => MultisigMemberPublicKey::MlDsa65(MlDsa65PublicKey {
                bytes: Some(Base64(pk.as_ref().to_vec())),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
    use sui_types::crypto::Signature;
    use sui_types::multisig::MultiSigPublicKey;
    use sui_types::utils::load_zklogin_v2_test_vector;
    use sui_types::utils::sign_zklogin_personal_msg;

    use super::MultiSig;
    use super::MultisigMemberPublicKey;
    use super::MultisigSignature;
    use super::SignatureScheme;
    use super::ZkLoginAuthenticatorV2;

    #[test]
    fn multisig_keeps_zklogin_member_details() {
        let message = PersonalMessage { message: vec![] };
        let (_, v1) = sign_zklogin_personal_msg(message.clone());
        let (signer, public_key, inputs, _, max_epoch) = load_zklogin_v2_test_vector();
        let intent = IntentMessage::new(Intent::personal_message(), message);
        let authenticator =
            ZkLoginAuthenticatorV2::new(inputs, max_epoch, Signature::new_secure(&intent, &signer))
                .unwrap();
        let committee =
            MultiSigPublicKey::new(vec![v1.to_public_key().unwrap(), public_key], vec![1, 1], 2)
                .unwrap();
        let multisig =
            MultiSig::combine(vec![v1.clone(), authenticator.clone().into()], committee).unwrap();

        let signature = MultisigSignature::from(&multisig);
        let [
            SignatureScheme::ZkLogin(v1_member),
            SignatureScheme::ZkLoginV2(v2_member),
        ] = signature.signatures.as_deref().unwrap()
        else {
            panic!("expected both zkLogin member signatures");
        };
        assert_eq!(v1_member.native.as_ref(), v1.as_ref());
        assert_eq!(v2_member.native.as_ref(), authenticator.as_ref());
        assert_eq!(signature.bitmap, Some(multisig.get_bitmap()));

        let committee = signature.committee.unwrap();
        let members = committee.members.unwrap();
        let Some(MultisigMemberPublicKey::ZkLogin(identifier)) = &members[1].public_key else {
            panic!("expected a zkLogin public identifier");
        };
        assert_eq!(identifier.iss.as_deref(), Some(authenticator.get_iss()));
        assert!(identifier.address_seed.is_some());
    }
}
