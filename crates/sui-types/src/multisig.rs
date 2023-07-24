// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    crypto::{CompressedSignature, DefaultHash, SignatureScheme},
    signature::{AuthenticatorTrait, VerifyParams},
};
pub use enum_dispatch::enum_dispatch;
use fastcrypto::{
    ed25519::Ed25519PublicKey,
    encoding::{Base64, Encoding},
    error::FastCryptoError,
    hash::HashFunction,
    secp256k1::Secp256k1PublicKey,
    secp256r1::Secp256r1PublicKey,
    traits::{ToFromBytes, VerifyingKey},
};
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::IntentMessage;
use std::{
    hash::{Hash, Hasher},
    str::FromStr,
};

use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{PublicKey, Signature},
    error::SuiError,
};

#[cfg(test)]
#[path = "unit_tests/multisig_tests.rs"]
mod multisig_tests;

pub type WeightUnit = u8;
pub type ThresholdUnit = u16;
pub type BitmapUnit = u16;
pub const MAX_SIGNER_IN_MULTISIG: usize = 10;
pub const MAX_BITMAP_VALUE: BitmapUnit = 0b1111111111;
/// The struct that contains signatures and public keys necessary for authenticating a MultiSig.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct MultiSig {
    /// The plain signature encoded with signature scheme.
    sigs: Vec<CompressedSignature>,
    /// A bitmap that indicates the position of which public key the signature should be authenticated with.
    bitmap: BitmapUnit,
    /// The public key encoded with each public key with its signature scheme used along with the corresponding weight.
    multisig_pk: MultiSigPublicKey,
    /// A bytes representation of [struct MultiSig]. This helps with implementing [trait AsRef<[u8]>].
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for MultiSig {
    fn eq(&self, other: &Self) -> bool {
        self.sigs == other.sigs
            && self.bitmap == other.bitmap
            && self.multisig_pk == other.multisig_pk
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for MultiSig {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for MultiSig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for MultiSig {
    fn verify_user_authenticator_epoch(&self, _: EpochId) -> Result<(), SuiError> {
        Ok(())
    }

    fn verify_claims<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        _aux_verify_data: &VerifyParams,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        if self.multisig_pk.pk_map.len() > MAX_SIGNER_IN_MULTISIG {
            return Err(SuiError::InvalidSignature {
                error: "Invalid number of public keys".to_string(),
            });
        }

        if SuiAddress::from(&self.multisig_pk) != author {
            return Err(SuiError::InvalidSignature {
                error: "Invalid address".to_string(),
            });
        }
        let mut weight_sum: u16 = 0;
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");
        let mut hasher = DefaultHash::default();
        hasher.update(message);
        let digest = hasher.finalize().digest;

        // Verify each signature against its corresponding signature scheme and public key.
        // TODO: further optimization can be done because multiple Ed25519 signatures can be batch verified.
        for (sig, i) in self.sigs.iter().zip(as_indices(self.bitmap)?) {
            let (pk, weight) =
                self.multisig_pk
                    .pk_map
                    .get(i as usize)
                    .ok_or(SuiError::InvalidSignature {
                        error: "Invalid public keys index".to_string(),
                    })?;
            let res = match sig {
                CompressedSignature::Ed25519(s) => {
                    let pk = Ed25519PublicKey::from_bytes(pk.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Fail to verify single sig".to_string(),
                        })?,
                    )
                }
                CompressedSignature::Secp256k1(s) => {
                    let pk = Secp256k1PublicKey::from_bytes(pk.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Fail to verify single sig".to_string(),
                        })?,
                    )
                }
                CompressedSignature::Secp256r1(s) => {
                    let pk = Secp256r1PublicKey::from_bytes(pk.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Fail to verify single sig".to_string(),
                        })?,
                    )
                }
            };
            if res.is_ok() {
                weight_sum += *weight as u16;
            } else {
                return Err(SuiError::InvalidSignature {
                    error: format!("Invalid signature for pk={:?}", pk),
                });
            }
        }
        if weight_sum >= self.multisig_pk.threshold {
            Ok(())
        } else {
            Err(SuiError::InvalidSignature {
                error: format!("Insufficient weight {:?}", weight_sum),
            })
        }
    }
}

/// Interpret a bitmap of 01s as a list of indices that is set to 1s.
/// e.g. 22 = 0b10110, then the result is [1, 2, 4].
pub fn as_indices(bitmap: u16) -> Result<Vec<u8>, SuiError> {
    if bitmap > MAX_BITMAP_VALUE {
        return Err(SuiError::InvalidSignature {
            error: "Invalid bitmap".to_string(),
        });
    }
    let mut res = Vec::new();
    for i in 0..10 {
        if bitmap & (1 << i) != 0 {
            res.push(i as u8);
        }
    }
    Ok(res)
}

impl MultiSig {
    /// Create MultiSig from its fields.
    pub fn new(
        sigs: Vec<CompressedSignature>,
        bitmap: u16,
        multisig_pk: MultiSigPublicKey,
    ) -> Self {
        Self {
            sigs,
            bitmap,
            multisig_pk,
            bytes: OnceCell::new(),
        }
    }
    /// This combines a list of [enum Signature] `flag || signature || pk` to a MultiSig.
    /// The order of full_sigs must be the same as the order of public keys in
    /// [enum MultiSigPublicKey]. e.g. for [pk1, pk2, pk3, pk4, pk5],
    /// [sig1, sig2, sig5] is valid, but [sig2, sig1, sig5] is invalid.
    pub fn combine(
        full_sigs: Vec<Signature>,
        multisig_pk: MultiSigPublicKey,
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
        let mut bitmap = 0;
        let mut sigs = Vec::with_capacity(full_sigs.len());
        for s in full_sigs {
            let pk = s.to_public_key()?;
            let index = multisig_pk
                .get_index(&pk)
                .ok_or(SuiError::IncorrectSigner {
                    error: format!("pk does not exist: {:?}", pk),
                })?;
            if bitmap & (1 << index) != 0 {
                return Err(SuiError::InvalidSignature {
                    error: "Duplicate public key".to_string(),
                });
            }
            bitmap |= 1 << index;
            sigs.push(s.to_compressed()?);
        }

        Ok(MultiSig {
            sigs,
            bitmap,
            multisig_pk,
            bytes: OnceCell::new(),
        })
    }

    pub fn validate(&self) -> Result<(), FastCryptoError> {
        if self.sigs.len() > self.multisig_pk.pk_map.len()
            || self.sigs.is_empty()
            || self.bitmap > MAX_BITMAP_VALUE
        {
            return Err(FastCryptoError::InvalidInput);
        }
        self.multisig_pk.validate()?;
        Ok(())
    }

    pub fn get_pk(&self) -> &MultiSigPublicKey {
        &self.multisig_pk
    }

    pub fn get_sigs(&self) -> &[CompressedSignature] {
        &self.sigs
    }

    pub fn get_indices(&self) -> Result<Vec<u8>, SuiError> {
        as_indices(self.bitmap)
    }
}

impl ToFromBytes for MultiSig {
    fn from_bytes(bytes: &[u8]) -> Result<MultiSig, FastCryptoError> {
        // The first byte matches the flag of MultiSig.
        if bytes.first().ok_or(FastCryptoError::InvalidInput)? != &SignatureScheme::MultiSig.flag()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        let multisig: MultiSig =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;
        multisig.validate()?;
        Ok(multisig)
    }
}

impl FromStr for MultiSig {
    type Err = SuiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = Base64::decode(s).map_err(|_| SuiError::InvalidSignature {
            error: "Invalid base64 string".to_string(),
        })?;
        let sig = MultiSig::from_bytes(&bytes).map_err(|_| SuiError::InvalidSignature {
            error: "Invalid multisig bytes".to_string(),
        })?;
        Ok(sig)
    }
}

/// This initialize the underlying bytes representation of MultiSig. It encodes
/// [struct MultiSig] as the MultiSig flag (0x03) concat with the bcs bytes
/// of [struct MultiSig] i.e. `flag || bcs_bytes(MultiSig)`.
impl AsRef<[u8]> for MultiSig {
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

/// The struct that contains the public key used for authenticating a MultiSig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MultiSigPublicKey {
    /// A list of public key and its corresponding weight.
    pk_map: Vec<(PublicKey, WeightUnit)>,
    /// If the total weight of the public keys corresponding to verified signatures is larger than threshold, the MultiSig is verified.
    threshold: ThresholdUnit,
}

impl MultiSigPublicKey {
    /// Construct MultiSigPublicKey from its fields.
    pub fn construct(pk_map: Vec<(PublicKey, WeightUnit)>, threshold: ThresholdUnit) -> Self {
        Self { pk_map, threshold }
    }

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
        Ok(MultiSigPublicKey {
            pk_map: pks.into_iter().zip(weights.into_iter()).collect(),
            threshold,
        })
    }

    pub fn get_index(&self, pk: &PublicKey) -> Option<u8> {
        self.pk_map.iter().position(|x| &x.0 == pk).map(|x| x as u8)
    }

    pub fn threshold(&self) -> &ThresholdUnit {
        &self.threshold
    }

    pub fn pubkeys(&self) -> &Vec<(PublicKey, WeightUnit)> {
        &self.pk_map
    }

    pub fn validate(&self) -> Result<(), FastCryptoError> {
        let pk_map = self.pubkeys();
        if self.threshold == 0
            || pk_map.is_empty()
            || pk_map.len() > MAX_SIGNER_IN_MULTISIG
            || pk_map.iter().any(|(_pk, weight)| *weight == 0)
            || pk_map
                .iter()
                .map(|(_pk, weight)| *weight as ThresholdUnit)
                .sum::<ThresholdUnit>()
                < self.threshold
        {
            return Err(FastCryptoError::InvalidInput);
        }
        Ok(())
    }
}
