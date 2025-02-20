use super::TryFromProtoError;
use bytes::{BufMut, BytesMut};
use tap::Pipe;

//
// ValidatorAggregatedSignature
//

impl From<sui_sdk_types::ValidatorAggregatedSignature> for super::ValidatorAggregatedSignature {
    fn from(value: sui_sdk_types::ValidatorAggregatedSignature) -> Self {
        Self {
            epoch: Some(value.epoch),
            signature: Some(value.signature.as_bytes().to_vec().into()),
            bitmap: Some(value.bitmap.into()),
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
        let bitmap = value
            .bitmap
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("bitmap"))?
            .try_into()?;

        Ok(Self {
            epoch,
            signature,
            bitmap,
        })
    }
}

//
// RoaringBitmap
//

impl From<roaring::RoaringBitmap> for super::RoaringBitmap {
    fn from(value: roaring::RoaringBitmap) -> Self {
        Self::from(&value)
    }
}

impl From<&roaring::RoaringBitmap> for super::RoaringBitmap {
    fn from(value: &roaring::RoaringBitmap) -> Self {
        let mut buf = BytesMut::new().writer();
        value
            .serialize_into(&mut buf)
            .expect("writing to BytesMut can't fail");
        Self {
            bitmap: Some(buf.into_inner().freeze()),
        }
    }
}

impl TryFrom<&super::RoaringBitmap> for roaring::RoaringBitmap {
    type Error = TryFromProtoError;

    fn try_from(value: &super::RoaringBitmap) -> Result<Self, Self::Error> {
        value
            .bitmap
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("bitmap"))?
            .as_ref()
            .pipe(roaring::RoaringBitmap::deserialize_from)
            .map_err(TryFromProtoError::from_error)
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
// Bn254FieldElement
//

impl From<sui_sdk_types::Bn254FieldElement> for super::Bn254FieldElement {
    fn from(value: sui_sdk_types::Bn254FieldElement) -> Self {
        Self {
            element: Some(value.padded().to_vec().into()),
        }
    }
}

impl TryFrom<&super::Bn254FieldElement> for sui_sdk_types::Bn254FieldElement {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Bn254FieldElement) -> Result<Self, Self::Error> {
        Ok(Self::new(
            value
                .element
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("element"))?
                .as_ref()
                .try_into()
                .map_err(TryFromProtoError::from_error)?,
        ))
    }
}

//
// CircomG1
//

impl From<sui_sdk_types::CircomG1> for super::CircomG1 {
    fn from(value: sui_sdk_types::CircomG1) -> Self {
        let [e0, e1, e2] = value.0;

        Self {
            e0: Some(e0.into()),
            e1: Some(e1.into()),
            e2: Some(e2.into()),
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
            .try_into()?;
        let e1 = value
            .e1
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e1"))?
            .try_into()?;
        let e2 = value
            .e2
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e2"))?
            .try_into()?;

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
            e00: Some(e00.into()),
            e01: Some(e01.into()),
            e10: Some(e10.into()),
            e11: Some(e11.into()),
            e20: Some(e20.into()),
            e21: Some(e21.into()),
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
            .try_into()?;
        let e01 = value
            .e01
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e01"))?
            .try_into()?;

        let e10 = value
            .e10
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e10"))?
            .try_into()?;
        let e11 = value
            .e11
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e11"))?
            .try_into()?;

        let e20 = value
            .e20
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e20"))?
            .try_into()?;
        let e21 = value
            .e21
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("e21"))?
            .try_into()?;

        Ok(Self([[e00, e01], [e10, e11], [e20, e21]]))
    }
}

//
// ZkLoginClaim
//

impl From<sui_sdk_types::Claim> for super::ZkLoginClaim {
    fn from(sui_sdk_types::Claim { value, index_mod_4 }: sui_sdk_types::Claim) -> Self {
        Self {
            value: Some(value),
            index_mod_4: Some(index_mod_4.into()),
        }
    }
}

impl TryFrom<&super::ZkLoginClaim> for sui_sdk_types::Claim {
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
            address_seed: Some(address_seed.into()),
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
            .try_into()?;

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
            signature: Some(value.signature.into()),
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
            address_seed: Some(value.address_seed().to_owned().into()),
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
            .try_into()?;

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

impl From<sui_sdk_types::SimpleSignature> for super::SimpleSignature {
    fn from(value: sui_sdk_types::SimpleSignature) -> Self {
        let scheme: super::SignatureScheme = value.scheme().into();
        let (signature, public_key) = match &value {
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

        Self {
            scheme: Some(scheme.into()),
            signature: Some(signature.to_vec().into()),
            public_key: Some(public_key.to_vec().into()),
        }
    }
}

impl TryFrom<&super::SimpleSignature> for sui_sdk_types::SimpleSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::SimpleSignature) -> Result<Self, Self::Error> {
        use super::SignatureScheme;
        use super::SignatureScheme::*;
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
            Ed25519 => Self::Ed25519 {
                signature: Ed25519Signature::from_bytes(signature)?,
                public_key: Ed25519PublicKey::from_bytes(public_key)?,
            },
            Secp256k1 => Self::Secp256k1 {
                signature: Secp256k1Signature::from_bytes(signature)?,
                public_key: Secp256k1PublicKey::from_bytes(public_key)?,
            },
            Secp256r1 => Self::Secp256r1 {
                signature: Secp256r1Signature::from_bytes(signature)?,
                public_key: Secp256r1PublicKey::from_bytes(public_key)?,
            },
            Multisig | Bls12381 | Zklogin | Passkey => {
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
            signature: Some(value.signature().into()),
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
        use super::multisig_member_public_key::Scheme;
        use sui_sdk_types::MultisigMemberPublicKey::*;

        let scheme = match value {
            Ed25519(public_key) => Scheme::Ed25519(public_key.as_bytes().to_vec().into()),
            Secp256k1(public_key) => Scheme::Secp256k1(public_key.as_bytes().to_vec().into()),
            Secp256r1(public_key) => Scheme::Secp256r1(public_key.as_bytes().to_vec().into()),
            ZkLogin(zklogin_id) => Scheme::Zklogin(zklogin_id.into()),
        };

        Self {
            scheme: Some(scheme),
        }
    }
}

impl TryFrom<&super::MultisigMemberPublicKey> for sui_sdk_types::MultisigMemberPublicKey {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigMemberPublicKey) -> Result<Self, Self::Error> {
        use super::multisig_member_public_key::Scheme;
        use sui_sdk_types::{Ed25519PublicKey, Secp256k1PublicKey, Secp256r1PublicKey};

        match value
            .scheme
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("scheme"))?
        {
            Scheme::Ed25519(public_key) => Self::Ed25519(Ed25519PublicKey::from_bytes(public_key)?),
            Scheme::Secp256k1(public_key) => {
                Self::Secp256k1(Secp256k1PublicKey::from_bytes(public_key)?)
            }
            Scheme::Secp256r1(public_key) => {
                Self::Secp256r1(Secp256r1PublicKey::from_bytes(public_key)?)
            }
            Scheme::Zklogin(zklogin_id) => Self::ZkLogin(zklogin_id.try_into()?),
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
        use super::multisig_member_signature::Signature;
        use sui_sdk_types::MultisigMemberSignature::*;

        let signature = match value {
            Ed25519(signautre) => Signature::Ed25519(signautre.as_bytes().to_vec().into()),
            Secp256k1(signautre) => Signature::Secp256k1(signautre.as_bytes().to_vec().into()),
            Secp256r1(signautre) => Signature::Secp256r1(signautre.as_bytes().to_vec().into()),
            ZkLogin(zklogin_id) => Signature::Zklogin((**zklogin_id).clone().into()),
        };

        Self {
            signature: Some(signature),
        }
    }
}

impl TryFrom<&super::MultisigMemberSignature> for sui_sdk_types::MultisigMemberSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MultisigMemberSignature) -> Result<Self, Self::Error> {
        use super::multisig_member_signature::Signature;
        use sui_sdk_types::{Ed25519Signature, Secp256k1Signature, Secp256r1Signature};

        match value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?
        {
            Signature::Ed25519(signautre) => {
                Self::Ed25519(Ed25519Signature::from_bytes(signautre)?)
            }
            Signature::Secp256k1(signautre) => {
                Self::Secp256k1(Secp256k1Signature::from_bytes(signautre)?)
            }
            Signature::Secp256r1(signautre) => {
                Self::Secp256r1(Secp256r1Signature::from_bytes(signautre)?)
            }
            Signature::Zklogin(zklogin_id) => Self::ZkLogin(Box::new(zklogin_id.try_into()?)),
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
            legacy_bitmap: value.legacy_bitmap().map(Into::into),
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
        let legacy_bitmap = value
            .legacy_bitmap
            .as_ref()
            .map(TryInto::try_into)
            .transpose()?;
        let committee = value
            .committee
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("committee"))?
            .try_into()?;

        let mut signature = Self::new(committee, signatures, bitmap);

        if let Some(legacy_bitmap) = legacy_bitmap {
            signature.with_legacy_bitmap(legacy_bitmap);
        }

        Ok(signature)
    }
}

//
// UserSignature
//

impl From<sui_sdk_types::UserSignature> for super::UserSignature {
    fn from(value: sui_sdk_types::UserSignature) -> Self {
        use super::user_signature::Signature;
        use sui_sdk_types::UserSignature::*;

        let signature = match value {
            Simple(simple) => Signature::Simple(simple.into()),
            Multisig(ref multisig) => Signature::Multisig(multisig.into()),
            ZkLogin(zklogin) => Signature::Zklogin((*zklogin).into()),
            Passkey(passkey) => Signature::Passkey(passkey.into()),
        };

        Self {
            signature: Some(signature),
        }
    }
}

impl TryFrom<&super::UserSignature> for sui_sdk_types::UserSignature {
    type Error = TryFromProtoError;

    fn try_from(value: &super::UserSignature) -> Result<Self, Self::Error> {
        use super::user_signature::Signature;

        match value
            .signature
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("signature"))?
        {
            Signature::Simple(simple) => Self::Simple(simple.try_into()?),
            Signature::Multisig(multisig) => Self::Multisig(multisig.try_into()?),
            Signature::Zklogin(zklogin) => Self::ZkLogin(Box::new(zklogin.try_into()?)),
            Signature::Passkey(passkey) => Self::Passkey(passkey.try_into()?),
        }
        .pipe(Ok)
    }
}
