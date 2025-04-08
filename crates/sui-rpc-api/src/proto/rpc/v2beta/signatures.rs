// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::SignatureScheme;
use crate::{
    message::{MessageField, MessageFields, MessageMerge},
    proto::TryFromProtoError,
};
use tap::Pipe;

//
// ValidatorAggregatedSignature
//

impl From<sui_sdk_types::ValidatorAggregatedSignature> for super::ValidatorAggregatedSignature {
    fn from(value: sui_sdk_types::ValidatorAggregatedSignature) -> Self {
        Self {
            epoch: Some(value.epoch),
            signature: Some(value.signature.as_bytes().to_vec().into()),
            bitmap: value.bitmap.iter().collect(),
        }
    }
}

impl TryFrom<&super::ValidatorAggregatedSignature> for sui_sdk_types::ValidatorAggregatedSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ValidatorAggregatedSignature) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let signature = value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?
            .as_ref()
            .pipe(sui_sdk_types::Bls12381Signature::from_bytes)
            .map_err(TryFromProtoError::from_error)?;
        let bitmap = value.bitmap.iter().copied().collect();

        Ok(Self {
            epoch,
            signature,
            bitmap,
        })
    }
}

//
// ValidatorCommitteeMember
//

impl From<sui_sdk_types::ValidatorCommitteeMember> for super::ValidatorCommitteeMember {
    fn from(value: sui_sdk_types::ValidatorCommitteeMember) -> Self {
        Self {
            public_key: Some(value.public_key.as_bytes().to_vec().into()),
            stake: Some(value.stake),
        }
    }
}

impl TryFrom<&super::ValidatorCommitteeMember> for sui_sdk_types::ValidatorCommitteeMember {
    type Error = TryFromProtoError;

    fn try_from(
        super::ValidatorCommitteeMember { public_key, stake }: &super::ValidatorCommitteeMember,
    ) -> Result<Self, Self::Error> {
        let public_key = public_key
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("public_key"))?
            .as_ref()
            .pipe(sui_sdk_types::Bls12381PublicKey::from_bytes)
            .map_err(TryFromProtoError::from_error)?;
        let stake = stake.ok_or_else(|| TryFromProtoError::missing("stake"))?;
        Ok(Self { public_key, stake })
    }
}

//
// ValidatorCommittee
//

impl From<sui_sdk_types::ValidatorCommittee> for super::ValidatorCommittee {
    fn from(value: sui_sdk_types::ValidatorCommittee) -> Self {
        Self {
            epoch: Some(value.epoch),
            members: value.members.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::ValidatorCommittee> for sui_sdk_types::ValidatorCommittee {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ValidatorCommittee) -> Result<Self, Self::Error> {
        let epoch = value
            .epoch
            .ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        Ok(Self {
            epoch,
            members: value
                .members
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// CircomG1
//

impl From<sui_sdk_types::CircomG1> for super::CircomG1 {
    fn from(value: sui_sdk_types::CircomG1) -> Self {
        let [e0, e1, e2] = value.0;

        Self {
            e0: Some(e0.to_string()),
            e1: Some(e1.to_string()),
            e2: Some(e2.to_string()),
        }
    }
}

impl TryFrom<&super::CircomG1> for sui_sdk_types::CircomG1 {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CircomG1) -> Result<Self, Self::Error> {
        let e0 = value
            .e0
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e0"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let e1 = value
            .e1
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e1"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let e2 = value
            .e2
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e2"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self([e0, e1, e2]))
    }
}

//
// CircomG2
//

impl From<sui_sdk_types::CircomG2> for super::CircomG2 {
    fn from(value: sui_sdk_types::CircomG2) -> Self {
        let [[e00, e01], [e10, e11], [e20, e21]] = value.0;

        Self {
            e00: Some(e00.to_string()),
            e01: Some(e01.to_string()),
            e10: Some(e10.to_string()),
            e11: Some(e11.to_string()),
            e20: Some(e20.to_string()),
            e21: Some(e21.to_string()),
        }
    }
}

impl TryFrom<&super::CircomG2> for sui_sdk_types::CircomG2 {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CircomG2) -> Result<Self, Self::Error> {
        let e00 = value
            .e00
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e00"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let e01 = value
            .e01
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e01"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let e10 = value
            .e10
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e10"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let e11 = value
            .e11
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e11"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let e20 = value
            .e20
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e20"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let e21 = value
            .e21
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e21"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self([[e00, e01], [e10, e11], [e20, e21]]))
    }
}

//
// ZkLoginClaim
//

impl From<sui_sdk_types::ZkLoginClaim> for super::ZkLoginClaim {
    fn from(
        sui_sdk_types::ZkLoginClaim { value, index_mod_4 }: sui_sdk_types::ZkLoginClaim,
    ) -> Self {
        Self {
            value: Some(value),
            index_mod_4: Some(index_mod_4.into()),
        }
    }
}

impl TryFrom<&super::ZkLoginClaim> for sui_sdk_types::ZkLoginClaim {
    type Error = TryFromProtoError;

    fn try_from(
        super::ZkLoginClaim { value, index_mod_4 }: &super::ZkLoginClaim,
    ) -> Result<Self, Self::Error> {
        let value = value
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("value"))?
            .into();
        let index_mod_4 = index_mod_4
            .ok_or_else(|| TryFromProtoError::missing("index_mod_4"))?
            .try_into()?;

        Ok(Self { value, index_mod_4 })
    }
}

//
// ZkLoginProof
//

impl From<sui_sdk_types::ZkLoginProof> for super::ZkLoginProof {
    fn from(value: sui_sdk_types::ZkLoginProof) -> Self {
        Self {
            a: Some(value.a.into()),
            b: Some(value.b.into()),
            c: Some(value.c.into()),
        }
    }
}

impl TryFrom<&super::ZkLoginProof> for sui_sdk_types::ZkLoginProof {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ZkLoginProof) -> Result<Self, Self::Error> {
        let a = value
            .a
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("a"))?
            .try_into()?;
        let b = value
            .b
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("b"))?
            .try_into()?;
        let c = value
            .c
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("c"))?
            .try_into()?;

        Ok(Self { a, b, c })
    }
}

//
// ZkLoginInputs
//

impl From<sui_sdk_types::ZkLoginInputs> for super::ZkLoginInputs {
    fn from(
        sui_sdk_types::ZkLoginInputs {
            proof_points,
            iss_base64_details,
            header_base64,
            address_seed,
        }: sui_sdk_types::ZkLoginInputs,
    ) -> Self {
        Self {
            proof_points: Some(proof_points.into()),
            iss_base64_details: Some(iss_base64_details.into()),
            header_base64: Some(header_base64),
            address_seed: Some(address_seed.to_string()),
        }
    }
}

impl TryFrom<&super::ZkLoginInputs> for sui_sdk_types::ZkLoginInputs {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ZkLoginInputs) -> Result<Self, Self::Error> {
        let proof_points = value
            .proof_points
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("proof_points"))?
            .try_into()?;
        let iss_base64_details = value
            .iss_base64_details
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("iss_base64_details"))?
            .try_into()?;
        let header_base64 = value
            .header_base64
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("header_base64"))?
            .into();
        let address_seed = value
            .address_seed
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address_seed"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Ok(Self {
            proof_points,
            iss_base64_details,
            header_base64,
            address_seed,
        })
    }
}

//
// ZkLoginAuthenticator
//

impl From<sui_sdk_types::ZkLoginAuthenticator> for super::ZkLoginAuthenticator {
    fn from(value: sui_sdk_types::ZkLoginAuthenticator) -> Self {
        Self {
            inputs: Some(value.inputs.into()),
            max_epoch: Some(value.max_epoch),
            signature: Some(Box::new(value.signature.into())),
        }
    }
}

impl TryFrom<&super::ZkLoginAuthenticator> for sui_sdk_types::ZkLoginAuthenticator {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ZkLoginAuthenticator) -> Result<Self, Self::Error> {
        let inputs = value
            .inputs
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("inputs"))?
            .try_into()?;
        let max_epoch = value
            .max_epoch
            .ok_or_else(|| TryFromProtoError::missing("max_epoch"))?;
        let signature = value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?
            .as_ref()
            .try_into()?;

        Ok(Self {
            inputs,
            max_epoch,
            signature,
        })
    }
}

//
// ZkLoginPublicIdentifier
//

impl From<&sui_sdk_types::ZkLoginPublicIdentifier> for super::ZkLoginPublicIdentifier {
    fn from(value: &sui_sdk_types::ZkLoginPublicIdentifier) -> Self {
        Self {
            iss: Some(value.iss().to_owned()),
            address_seed: Some(value.address_seed().to_string()),
        }
    }
}

impl TryFrom<&super::ZkLoginPublicIdentifier> for sui_sdk_types::ZkLoginPublicIdentifier {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ZkLoginPublicIdentifier) -> Result<Self, Self::Error> {
        let iss = value
            .iss
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("iss"))?
            .into();
        let address_seed = value
            .address_seed
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address_seed"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        Self::new(iss, address_seed)
            .ok_or_else(|| TryFromProtoError::from_error("invalid iss"))?
            .pipe(Ok)
    }
}

//
// SignatureScheme
//

impl From<sui_sdk_types::SignatureScheme> for super::SignatureScheme {
    fn from(value: sui_sdk_types::SignatureScheme) -> Self {
        use sui_sdk_types::SignatureScheme::*;

        match value {
            Ed25519 => Self::Ed25519,
            Secp256k1 => Self::Secp256k1,
            Secp256r1 => Self::Secp256r1,
            Multisig => Self::Multisig,
            Bls12381 => Self::Bls12381,
            ZkLogin => Self::Zklogin,
            Passkey => Self::Passkey,
        }
    }
}

impl TryFrom<&super::SignatureScheme> for sui_sdk_types::SignatureScheme {
    type Error = TryFromProtoError;

    fn try_from(value: &super::SignatureScheme) -> Result<Self, Self::Error> {
        use super::SignatureScheme::*;

        match value {
            Ed25519 => Self::Ed25519,
            Secp256k1 => Self::Secp256k1,
            Secp256r1 => Self::Secp256r1,
            Multisig => Self::Multisig,
            Bls12381 => Self::Bls12381,
            Zklogin => Self::ZkLogin,
            Passkey => Self::Passkey,
        }
        .pipe(Ok)
    }
}

//
// SimpleSignature
//

impl From<sui_sdk_types::SimpleSignature> for super::UserSignature {
    fn from(value: sui_sdk_types::SimpleSignature) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::SimpleSignature> for super::UserSignature {
    fn merge(
        &mut self,
        source: sui_sdk_types::SimpleSignature,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        let scheme: super::SignatureScheme = source.scheme().into();
        let (signature, public_key) = match &source {
            sui_sdk_types::SimpleSignature::Ed25519 {
                signature,
                public_key,
            } => (signature.as_bytes(), public_key.as_bytes()),
            sui_sdk_types::SimpleSignature::Secp256k1 {
                signature,
                public_key,
            } => (signature.as_bytes(), public_key.as_bytes()),
            sui_sdk_types::SimpleSignature::Secp256r1 {
                signature,
                public_key,
            } => (signature.as_bytes(), public_key.as_bytes()),
        };

        if mask.contains(Self::SCHEME_FIELD.name) {
            self.set_scheme(scheme);
        }
        if mask.contains(Self::SIGNATURE_FIELD.name) {
            self.signature = Some(signature.to_vec().into());
        }
        if mask.contains(Self::PUBLIC_KEY_FIELD.name) {
            self.public_key = Some(public_key.to_vec().into());
        }
    }
}

impl TryFrom<&super::UserSignature> for sui_sdk_types::SimpleSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::UserSignature) -> Result<Self, Self::Error> {
        use super::SignatureScheme;
        use sui_sdk_types::{Ed25519PublicKey, Ed25519Signature};
        use sui_sdk_types::{
            Secp256k1PublicKey, Secp256k1Signature, Secp256r1PublicKey, Secp256r1Signature,
        };

        let signature = value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?;
        let public_key = value
            .public_key
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("public_key"))?;
        let scheme = value
            .scheme
            .ok_or_else(|| TryFromProtoError::missing("scheme"))?
            .pipe(SignatureScheme::try_from)
            .map_err(TryFromProtoError::from_error)?;

        match scheme {
            SignatureScheme::Ed25519 => Self::Ed25519 {
                signature: Ed25519Signature::from_bytes(signature)?,
                public_key: Ed25519PublicKey::from_bytes(public_key)?,
            },
            SignatureScheme::Secp256k1 => Self::Secp256k1 {
                signature: Secp256k1Signature::from_bytes(signature)?,
                public_key: Secp256k1PublicKey::from_bytes(public_key)?,
            },
            SignatureScheme::Secp256r1 => Self::Secp256r1 {
                signature: Secp256r1Signature::from_bytes(signature)?,
                public_key: Secp256r1PublicKey::from_bytes(public_key)?,
            },
            SignatureScheme::Multisig
            | SignatureScheme::Bls12381
            | SignatureScheme::Zklogin
            | SignatureScheme::Passkey => {
                return Err(TryFromProtoError::from_error(
                    "invalid or unknown signature scheme",
                ))
            }
        }
        .pipe(Ok)
    }
}

//
// PasskeyAuthenticator
//

impl From<sui_sdk_types::PasskeyAuthenticator> for super::PasskeyAuthenticator {
    fn from(value: sui_sdk_types::PasskeyAuthenticator) -> Self {
        Self {
            authenticator_data: Some(value.authenticator_data().to_vec().into()),
            client_data_json: Some(value.client_data_json().to_owned()),
            signature: Some(Box::new(value.signature().into())),
        }
    }
}

impl TryFrom<&super::PasskeyAuthenticator> for sui_sdk_types::PasskeyAuthenticator {
    type Error = TryFromProtoError;

    fn try_from(value: &super::PasskeyAuthenticator) -> Result<Self, Self::Error> {
        let authenticator_data = value
            .authenticator_data
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("authenticator_data"))?
            .to_vec();
        let client_data_json = value
            .client_data_json
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("client_data_json"))?
            .into();

        let signature = value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?
            .as_ref()
            .try_into()?;

        Self::new(authenticator_data, client_data_json, signature)
            .ok_or_else(|| TryFromProtoError::from_error("invalid passkey"))
    }
}

//
// MultisigMemberPublicKey
//

impl From<&sui_sdk_types::MultisigMemberPublicKey> for super::MultisigMemberPublicKey {
    fn from(value: &sui_sdk_types::MultisigMemberPublicKey) -> Self {
        use sui_sdk_types::MultisigMemberPublicKey::*;

        let mut message = Self::default();

        let scheme = match value {
            Ed25519(public_key) => {
                message.public_key = Some(public_key.as_bytes().to_vec().into());
                SignatureScheme::Ed25519
            }
            Secp256k1(public_key) => {
                message.public_key = Some(public_key.as_bytes().to_vec().into());
                SignatureScheme::Secp256k1
            }
            Secp256r1(public_key) => {
                message.public_key = Some(public_key.as_bytes().to_vec().into());
                SignatureScheme::Secp256r1
            }
            ZkLogin(zklogin_id) => {
                message.zklogin = Some(zklogin_id.into());
                SignatureScheme::Zklogin
            }
        };

        message.set_scheme(scheme);
        message
    }
}

impl TryFrom<&super::MultisigMemberPublicKey> for sui_sdk_types::MultisigMemberPublicKey {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigMemberPublicKey) -> Result<Self, Self::Error> {
        use sui_sdk_types::{Ed25519PublicKey, Secp256k1PublicKey, Secp256r1PublicKey};

        match value.scheme() {
            SignatureScheme::Ed25519 => {
                Self::Ed25519(Ed25519PublicKey::from_bytes(value.public_key())?)
            }
            SignatureScheme::Secp256k1 => {
                Self::Secp256k1(Secp256k1PublicKey::from_bytes(value.public_key())?)
            }
            SignatureScheme::Secp256r1 => {
                Self::Secp256r1(Secp256r1PublicKey::from_bytes(value.public_key())?)
            }
            SignatureScheme::Zklogin => Self::ZkLogin(
                value
                    .zklogin
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("zklogin"))?
                    .try_into()?,
            ),
            SignatureScheme::Multisig | SignatureScheme::Bls12381 | SignatureScheme::Passkey => {
                return Err(TryFromProtoError::from_error(
                    "invalid MultisigMemberPublicKey scheme",
                ))
            }
        }
        .pipe(Ok)
    }
}

//
// MultisigMember
//

impl From<&sui_sdk_types::MultisigMember> for super::MultisigMember {
    fn from(value: &sui_sdk_types::MultisigMember) -> Self {
        Self {
            public_key: Some(value.public_key().into()),
            weight: Some(value.weight().into()),
        }
    }
}

impl TryFrom<&super::MultisigMember> for sui_sdk_types::MultisigMember {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigMember) -> Result<Self, Self::Error> {
        let public_key = value
            .public_key
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("public_key"))?
            .try_into()?;
        let weight = value
            .weight
            .ok_or_else(|| TryFromProtoError::missing("weight"))?
            .try_into()?;

        Ok(Self::new(public_key, weight))
    }
}

//
// MultisigCommittee
//

impl From<&sui_sdk_types::MultisigCommittee> for super::MultisigCommittee {
    fn from(value: &sui_sdk_types::MultisigCommittee) -> Self {
        Self {
            members: value.members().iter().map(Into::into).collect(),
            threshold: Some(value.threshold().into()),
        }
    }
}

impl TryFrom<&super::MultisigCommittee> for sui_sdk_types::MultisigCommittee {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigCommittee) -> Result<Self, Self::Error> {
        let members = value
            .members
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let threshold = value
            .threshold
            .ok_or_else(|| TryFromProtoError::missing("threshold"))?
            .try_into()?;

        Ok(Self::new(members, threshold))
    }
}

//
// MultisigMemberSignature
//

impl From<&sui_sdk_types::MultisigMemberSignature> for super::MultisigMemberSignature {
    fn from(value: &sui_sdk_types::MultisigMemberSignature) -> Self {
        use sui_sdk_types::MultisigMemberSignature::*;

        let mut message = Self::default();

        let scheme = match value {
            Ed25519(signature) => {
                message.signature = Some(signature.as_bytes().to_vec().into());
                SignatureScheme::Ed25519
            }
            Secp256k1(signature) => {
                message.signature = Some(signature.as_bytes().to_vec().into());
                SignatureScheme::Secp256k1
            }
            Secp256r1(signature) => {
                message.signature = Some(signature.as_bytes().to_vec().into());
                SignatureScheme::Secp256r1
            }
            ZkLogin(zklogin_id) => {
                message.zklogin = Some((**zklogin_id).clone().into());
                SignatureScheme::Zklogin
            }
        };

        message.set_scheme(scheme);
        message
    }
}

impl TryFrom<&super::MultisigMemberSignature> for sui_sdk_types::MultisigMemberSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigMemberSignature) -> Result<Self, Self::Error> {
        use sui_sdk_types::{Ed25519Signature, Secp256k1Signature, Secp256r1Signature};

        match value.scheme() {
            SignatureScheme::Ed25519 => {
                Self::Ed25519(Ed25519Signature::from_bytes(value.signature())?)
            }
            SignatureScheme::Secp256k1 => {
                Self::Secp256k1(Secp256k1Signature::from_bytes(value.signature())?)
            }
            SignatureScheme::Secp256r1 => {
                Self::Secp256r1(Secp256r1Signature::from_bytes(value.signature())?)
            }
            SignatureScheme::Zklogin => Self::ZkLogin(Box::new(
                value
                    .zklogin
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("zklogin"))?
                    .try_into()?,
            )),
            SignatureScheme::Multisig | SignatureScheme::Bls12381 | SignatureScheme::Passkey => {
                return Err(TryFromProtoError::from_error(
                    "invalid MultisigMemberSignature scheme",
                ))
            }
        }
        .pipe(Ok)
    }
}

//
// MultisigAggregatedSignature
//

impl From<&sui_sdk_types::MultisigAggregatedSignature> for super::MultisigAggregatedSignature {
    fn from(value: &sui_sdk_types::MultisigAggregatedSignature) -> Self {
        Self {
            signatures: value.signatures().iter().map(Into::into).collect(),
            bitmap: Some(value.bitmap().into()),
            legacy_bitmap: value
                .legacy_bitmap()
                .map(|roaring| roaring.iter().collect())
                .unwrap_or_default(),
            committee: Some(value.committee().into()),
        }
    }
}

impl TryFrom<&super::MultisigAggregatedSignature> for sui_sdk_types::MultisigAggregatedSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigAggregatedSignature) -> Result<Self, Self::Error> {
        let signatures = value
            .signatures
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let bitmap = value
            .bitmap
            .ok_or_else(|| TryFromProtoError::missing("bitmap"))?
            .try_into()?;
        let committee = value
            .committee
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("committee"))?
            .try_into()?;

        let mut signature = Self::new(committee, signatures, bitmap);

        if !value.legacy_bitmap.is_empty() {
            let legacy_bitmap = value
                .legacy_bitmap
                .iter()
                .copied()
                .collect::<roaring::RoaringBitmap>();
            signature.with_legacy_bitmap(legacy_bitmap);
        }

        Ok(signature)
    }
}

//
// UserSignature
//

impl super::UserSignature {
    const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(super::Bcs::FIELDS);
    const SCHEME_FIELD: &'static MessageField = &MessageField::new("scheme");
    const SIGNATURE_FIELD: &'static MessageField = &MessageField::new("signature");
    const PUBLIC_KEY_FIELD: &'static MessageField = &MessageField::new("public_key");
    const MULTISIG_FIELD: &'static MessageField = &MessageField::new("multisig");
    const ZKLOGIN_FIELD: &'static MessageField = &MessageField::new("zklogin");
    const PASSKEY_FIELD: &'static MessageField = &MessageField::new("passkey");
}

impl MessageFields for super::UserSignature {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::SCHEME_FIELD,
        Self::SIGNATURE_FIELD,
        Self::PUBLIC_KEY_FIELD,
        Self::MULTISIG_FIELD,
        Self::ZKLOGIN_FIELD,
        Self::PASSKEY_FIELD,
    ];
}

impl From<sui_sdk_types::UserSignature> for super::UserSignature {
    fn from(value: sui_sdk_types::UserSignature) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::UserSignature> for super::UserSignature {
    fn merge(
        &mut self,
        source: sui_sdk_types::UserSignature,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        use sui_sdk_types::UserSignature::*;

        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(super::Bcs {
                name: Some("UserSignatureBytes".to_owned()),
                value: Some(source.to_bytes().into()),
            });
        }

        if mask.contains(Self::SCHEME_FIELD.name) {
            let scheme: super::SignatureScheme = source.scheme().into();
            self.set_scheme(scheme);
        }

        match source {
            Simple(simple) => self.merge(simple, mask),
            Multisig(ref multisig) => {
                if mask.contains(Self::MULTISIG_FIELD.name) {
                    self.multisig = Some(multisig.into());
                }
            }
            ZkLogin(zklogin) => {
                if mask.contains(Self::ZKLOGIN_FIELD.name) {
                    self.zklogin = Some(Box::new((*zklogin).into()))
                }
            }
            Passkey(passkey) => {
                if mask.contains(Self::PASSKEY_FIELD.name) {
                    self.passkey = Some(Box::new(passkey.into()));
                }
            }
        }
    }
}

impl MessageMerge<&super::UserSignature> for super::UserSignature {
    fn merge(&mut self, source: &super::UserSignature, mask: &crate::field_mask::FieldMaskTree) {
        let super::UserSignature {
            bcs,
            scheme,
            signature,
            public_key,
            multisig,
            zklogin,
            passkey,
        } = source;

        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = bcs.clone();
        }

        if mask.contains(Self::SCHEME_FIELD.name) {
            self.scheme = *scheme;
        }

        if mask.contains(Self::SIGNATURE_FIELD.name) {
            self.signature = signature.clone();
        }

        if mask.contains(Self::PUBLIC_KEY_FIELD.name) {
            self.public_key = public_key.clone();
        }

        if mask.contains(Self::MULTISIG_FIELD.name) {
            self.multisig = multisig.clone();
        }

        if mask.contains(Self::ZKLOGIN_FIELD.name) {
            self.zklogin = zklogin.clone();
        }

        if mask.contains(Self::PASSKEY_FIELD.name) {
            self.passkey = passkey.clone();
        }
    }
}

impl TryFrom<&super::UserSignature> for sui_sdk_types::UserSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::UserSignature) -> Result<Self, Self::Error> {
        if let Some(bcs) = &value.bcs {
            if let Ok(sig) = Self::from_bytes(bcs.value()) {
                return Ok(sig);
            } else {
                return bcs.deserialize().map_err(TryFromProtoError::from_error);
            }
        }

        let scheme = value
            .scheme
            .ok_or_else(|| TryFromProtoError::missing("scheme"))?
            .pipe(SignatureScheme::try_from)
            .map_err(TryFromProtoError::from_error)?;

        match scheme {
            SignatureScheme::Ed25519 | SignatureScheme::Secp256k1 | SignatureScheme::Secp256r1 => {
                Self::Simple(value.try_into()?)
            }
            SignatureScheme::Multisig => Self::Multisig(
                value
                    .multisig
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("multisig"))?
                    .try_into()?,
            ),
            SignatureScheme::Zklogin => Self::ZkLogin(Box::new(
                value
                    .zklogin
                    .as_deref()
                    .ok_or_else(|| TryFromProtoError::missing("zklogin"))?
                    .try_into()?,
            )),
            SignatureScheme::Passkey => Self::Passkey(
                value
                    .passkey
                    .as_deref()
                    .ok_or_else(|| TryFromProtoError::missing("passkey"))?
                    .try_into()?,
            ),
            SignatureScheme::Bls12381 => {
                return Err(TryFromProtoError::from_error(
                    "invalid or unknown signature scheme",
                ))
            }
        }
        .pipe(Ok)
    }
}
