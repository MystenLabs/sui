// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    crypto::{CompressedSignature, SignatureScheme},
    digests::ZKLoginInputsDigest,
    multisig::{MultiSig, MultiSigPublicKey},
    signature::{AuthenticatorTrait, GenericSignature, VerifyParams},
    signature_verification::VerifiedDigestCache,
    sui_serde::SuiBitmap,
};
pub use enum_dispatch::enum_dispatch;
use fastcrypto::{
    encoding::Base64,
    error::FastCryptoError,
    traits::{EncodeDecodeBase64, ToFromBytes},
};
use once_cell::sync::OnceCell;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use shared_crypto::intent::IntentMessage;
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::PublicKey,
    error::SuiError,
};

pub type WeightUnit = u8;
pub type ThresholdUnit = u16;
pub const MAX_SIGNER_IN_MULTISIG: usize = 10;

/// Deprecated, use [struct MultiSig] instead.
/// The struct that contains signatures and public keys necessary for authenticating a MultiSigLegacy.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct MultiSigLegacy {
    /// The plain signature encoded with signature scheme.
    sigs: Vec<CompressedSignature>,
    /// A bitmap that indicates the position of which public key the signature should be authenticated with.
    #[schemars(with = "Base64")]
    #[serde_as(as = "SuiBitmap")]
    bitmap: RoaringBitmap,
    /// The public key encoded with each public key with its signature scheme used along with the corresponding weight.
    multisig_pk: MultiSigPublicKeyLegacy,
    /// A bytes representation of [struct MultiSigLegacy]. This helps with implementing [trait AsRef<[u8]>].
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// This initialize the underlying bytes representation of MultiSig. It encodes
/// [struct MultiSigLegacy] as the MultiSig flag (0x03) concat with the bcs bytes
/// of [struct MultiSigLegacy] i.e. `flag || bcs_bytes(MultiSig)`.
impl AsRef<[u8]> for MultiSigLegacy {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let as_bytes = bcs::to_bytes(self).expect("BCS serialization should not fail");
                let mut bytes = Vec::with_capacity(1 + as_bytes.len());
                bytes.push(SignatureScheme::MultiSig.flag());
                bytes.extend_from_slice(as_bytes.as_slice());
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for MultiSigLegacy {
    fn eq(&self, other: &Self) -> bool {
        self.sigs == other.sigs
            && self.bitmap == other.bitmap
            && self.multisig_pk == other.multisig_pk
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for MultiSigLegacy {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for MultiSigLegacy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for MultiSigLegacy {
    fn verify_user_authenticator_epoch(
        &self,
        epoch_id: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> Result<(), SuiError> {
        let multisig: MultiSig =
            self.clone()
                .try_into()
                .map_err(|_| SuiError::InvalidSignature {
                    error: "Invalid legacy multisig".to_string(),
                })?;
        multisig.verify_user_authenticator_epoch(epoch_id, max_epoch_upper_bound_delta)
    }

    fn verify_claims<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        aux_verify_data: &VerifyParams,
        zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        let multisig: MultiSig =
            self.clone()
                .try_into()
                .map_err(|_| SuiError::InvalidSignature {
                    error: "Invalid legacy multisig".to_string(),
                })?;
        multisig.verify_claims(value, author, aux_verify_data, zklogin_inputs_cache)
    }
}

impl TryFrom<MultiSigLegacy> for MultiSig {
    type Error = FastCryptoError;

    fn try_from(multisig: MultiSigLegacy) -> Result<Self, Self::Error> {
        MultiSig::insecure_new(
            multisig.clone().sigs,
            bitmap_to_u16(multisig.clone().bitmap)?,
            multisig.multisig_pk.try_into()?,
        )
        .init_and_validate()
    }
}

impl TryFrom<MultiSigPublicKeyLegacy> for MultiSigPublicKey {
    type Error = FastCryptoError;
    fn try_from(multisig: MultiSigPublicKeyLegacy) -> Result<Self, Self::Error> {
        let multisig_pk_legacy =
            MultiSigPublicKey::insecure_new(multisig.pk_map, multisig.threshold).validate()?;
        Ok(multisig_pk_legacy)
    }
}

/// Convert a roaring bitmap to plain bitmap.
pub fn bitmap_to_u16(roaring: RoaringBitmap) -> Result<u16, FastCryptoError> {
    let indices: Vec<u32> = roaring.into_iter().collect();
    let mut val = 0;
    for i in indices {
        if i >= 10 {
            return Err(FastCryptoError::InvalidInput);
        }
        val |= 1 << i as u8;
    }
    Ok(val)
}

impl MultiSigLegacy {
    /// This combines a list of [enum Signature] `flag || signature || pk` to a MultiSig.
    pub fn combine(
        full_sigs: Vec<GenericSignature>,
        multisig_pk: MultiSigPublicKeyLegacy,
    ) -> Result<Self, SuiError> {
        multisig_pk
            .validate()
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid multisig public key".to_string(),
            })?;

        if full_sigs.len() > multisig_pk.pk_map.len() || full_sigs.is_empty() {
            return Err(SuiError::InvalidSignature {
                error: "Invalid number of signatures".to_string(),
            });
        }
        let mut bitmap = RoaringBitmap::new();
        let mut sigs = Vec::with_capacity(full_sigs.len());
        for s in full_sigs {
            let pk = s.to_public_key()?;
            let inserted = bitmap.insert(multisig_pk.get_index(&pk).ok_or(
                SuiError::IncorrectSigner {
                    error: format!("pk does not exist: {:?}", pk),
                },
            )?);
            if !inserted {
                return Err(SuiError::InvalidSignature {
                    error: "Duplicate signature".to_string(),
                });
            }
            sigs.push(s.to_compressed()?);
        }
        Ok(MultiSigLegacy {
            sigs,
            bitmap,
            multisig_pk,
            bytes: OnceCell::new(),
        })
    }

    pub fn validate(&self) -> Result<(), FastCryptoError> {
        if bitmap_to_u16(self.bitmap.clone()).is_err()
            || self.sigs.len() > self.multisig_pk.pk_map.len()
            || self.sigs.is_empty()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        self.multisig_pk.validate()?;
        Ok(())
    }

    pub fn get_pk(&self) -> &MultiSigPublicKeyLegacy {
        &self.multisig_pk
    }

    pub fn get_sigs(&self) -> &[CompressedSignature] {
        &self.sigs
    }

    pub fn get_bitmap(&self) -> &RoaringBitmap {
        &self.bitmap
    }
}

impl ToFromBytes for MultiSigLegacy {
    fn from_bytes(bytes: &[u8]) -> Result<MultiSigLegacy, FastCryptoError> {
        // The first byte matches the flag of MultiSig.
        if bytes.first().ok_or(FastCryptoError::InvalidInput)? != &SignatureScheme::MultiSig.flag()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        let multisig: MultiSigLegacy =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidInput)?;
        multisig.validate()?;
        Ok(multisig)
    }
}

/// Deprecated, use [struct MultiSigPublicKey] instead.
/// The struct that contains the public key used for authenticating a MultiSig.
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Serialize, Deserialize)]
pub struct MultiSigPublicKeyLegacy {
    /// A list of public key and its corresponding weight.
    #[serde(serialize_with = "serialize_pk_map")]
    #[serde(deserialize_with = "deserialize_pk_map")]
    pk_map: Vec<(PublicKey, WeightUnit)>,
    /// If the total weight of the public keys corresponding to verified signatures is larger than threshold, the MultiSig is verified.
    threshold: ThresholdUnit,
}

/// Legacy serialization for MultiSigPublicKey where PublicKey is serialized as string in base64 encoding.
fn serialize_pk_map<S>(pk_map: &[(PublicKey, WeightUnit)], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let pk_weight_arr: Vec<(String, WeightUnit)> = pk_map
        .iter()
        .map(|(pk, w)| (pk.encode_base64(), *w))
        .collect();

    let mut seq = serializer.serialize_seq(Some(pk_weight_arr.len()))?;
    for (pk_string, w) in pk_weight_arr {
        seq.serialize_element(&(pk_string, w))?;
    }
    seq.end()
}

/// Legacy deserialization for MultiSigPublicKey where PublicKey is deserialized from base64 encoded string.
fn deserialize_pk_map<'de, D>(deserializer: D) -> Result<Vec<(PublicKey, WeightUnit)>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let pk_weight_arr: Vec<(String, WeightUnit)> = Vec::deserialize(deserializer)?;
    pk_weight_arr
        .into_iter()
        .map(|(s, w)| {
            let pk = <PublicKey as EncodeDecodeBase64>::decode_base64(&s)
                .map_err(|e| Error::custom(e.to_string()))?;
            Ok((pk, w))
        })
        .collect()
}

impl MultiSigPublicKeyLegacy {
    pub fn new(
        pks: Vec<PublicKey>,
        weights: Vec<WeightUnit>,
        threshold: ThresholdUnit,
    ) -> Result<Self, SuiError> {
        if pks.is_empty()
            || weights.is_empty()
            || threshold == 0
            || pks.len() != weights.len()
            || pks.len() > MAX_SIGNER_IN_MULTISIG
            || weights.iter().any(|w| *w == 0)
            || weights
                .iter()
                .map(|w| *w as ThresholdUnit)
                .sum::<ThresholdUnit>()
                < threshold
        {
            return Err(SuiError::InvalidSignature {
                error: "Invalid multisig public key construction".to_string(),
            });
        }
        Ok(MultiSigPublicKeyLegacy {
            pk_map: pks.into_iter().zip(weights).collect(),
            threshold,
        })
    }

    pub fn get_index(&self, pk: &PublicKey) -> Option<u32> {
        self.pk_map
            .iter()
            .position(|x| &x.0 == pk)
            .map(|x| x as u32)
    }

    pub fn threshold(&self) -> &ThresholdUnit {
        &self.threshold
    }

    pub fn pubkeys(&self) -> &[(PublicKey, WeightUnit)] {
        &self.pk_map
    }

    pub fn validate(&self) -> Result<Self, FastCryptoError> {
        let multisig: MultiSigPublicKey = self.clone().try_into()?;
        multisig.validate()?;
        Ok(self.clone())
    }
}
