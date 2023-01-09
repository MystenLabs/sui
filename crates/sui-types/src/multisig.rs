// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    crypto::{CompressedSignature, SignatureScheme, SuiSignature},
    serde_to_from_bytes,
    sui_serde::SuiBitmap,
};
pub use enum_dispatch::enum_dispatch;
use fastcrypto::{
    ed25519::Ed25519PublicKey,
    encoding::Base64,
    error::FastCryptoError,
    secp256k1::Secp256k1PublicKey,
    secp256r1::Secp256r1PublicKey,
    traits::{EncodeDecodeBase64, ToFromBytes},
    Verifier,
};
use once_cell::sync::OnceCell;
use roaring::RoaringBitmap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::hash::{Hash, Hasher};

use crate::{
    base_types::SuiAddress,
    crypto::{PublicKey, Signature},
    error::SuiError,
    intent::IntentMessage,
};

#[cfg(test)]
#[path = "unit_tests/multisig_tests.rs"]
mod multisig_tests;

pub type WeightUnit = u8;
pub type ThresholdUnit = u16;
pub const MAX_SIGNER_IN_MULTISIG: usize = 10;

/// A lightweight trait that all members of [enum GenericSignature] implement.
#[enum_dispatch]
pub trait AuthenticatorTrait {
    fn verify_secure_generic<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
    ) -> Result<(), SuiError>
    where
        T: Serialize;
}

/// Due to the incompatibility of [enum Signature] (which dispatches a trait that
/// assumes signature and pubkey bytes for verification), here we add a wrapper
/// enum where member can just implement a lightweight [trait AuthenticatorTrait].
/// This way multisig (and future Authenticators) can implement its own `verify`.
#[enum_dispatch(AuthenticatorTrait)]
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Hash)]
pub enum GenericSignature {
    MultiSignature,
    Signature,
}

/// GenericSignature encodes a single signature [enum Signature] as is `flag || signature || pubkey`.
/// It encodes [struct MultiSignature] as the multisig flag (0x03) concat with the bcs serializedbytes
/// of [struct MultiSignature] i.e. `flag || bcs_bytes(multisig)`.
impl ToFromBytes for GenericSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        match SignatureScheme::from_flag_byte(
            bytes.first().ok_or(FastCryptoError::InputTooShort(0))?,
        ) {
            Ok(x) => match x {
                SignatureScheme::Multisig => {
                    let multisig: MultiSignature =
                        bcs::from_bytes(bytes.get(1..).ok_or(FastCryptoError::InvalidInput)?)
                            .map_err(|_| FastCryptoError::InvalidSignature)?;
                    Ok(GenericSignature::MultiSignature(multisig))
                }
                SignatureScheme::Secp256k1
                | SignatureScheme::Secp256r1
                | SignatureScheme::ED25519 => Ok(GenericSignature::Signature(
                    <Signature as signature::Signature>::from_bytes(bytes)
                        .map_err(|_| FastCryptoError::InvalidSignature)?,
                )),
                _ => Err(FastCryptoError::InvalidInput),
            },
            Err(_) => Err(FastCryptoError::InvalidInput),
        }
    }
}

/// This initialize the underlying bytes representation of Multisig. It encodes
/// [struct MultiSignature] as the multisig flag (0x03) concat with the bcs bytes
/// of [struct MultiSignature] i.e. `flag || bcs_bytes(multisig)`.
impl AsRef<[u8]> for MultiSignature {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let mut bytes = Vec::new();
                bytes.push(SignatureScheme::Multisig.flag());
                bytes.extend_from_slice(
                    bcs::to_bytes(self)
                        .expect("BCS serialization should not fail")
                        .as_slice(),
                );
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

/// Trait useful to get the bytes reference for [enum GenericSignature].
impl AsRef<[u8]> for GenericSignature {
    fn as_ref(&self) -> &[u8] {
        match self {
            GenericSignature::MultiSignature(s) => s.as_ref(),
            GenericSignature::Signature(s) => s.as_ref(),
        }
    }
}

// A macro to implement [trait Serialize] and [trait Deserialize] for [enum GenericSignature] using its bytes representation.
serde_to_from_bytes!(GenericSignature);

/// The struct that contains signatures and public keys necessary for authenticating a multisig.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct MultiSignature {
    /// The plain signature encoded with signature scheme.
    sigs: Vec<CompressedSignature>,
    /// A bitmap that indicates the position of which public key the signature should be authenticated with.
    #[schemars(with = "Base64")]
    #[serde_as(as = "SuiBitmap")]
    bitmap: RoaringBitmap,
    /// The public key encoded with each public key with its signature scheme used along with the corresponding weight.
    multi_pk: MultiPublicKey,
    /// A bytes representation of [struct MultiSignature]. This helps with implementing [trait AsRef<[u8]>].
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for MultiSignature {
    fn eq(&self, other: &Self) -> bool {
        self.sigs == other.sigs && self.bitmap == other.bitmap && self.multi_pk == other.multi_pk
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for MultiSignature {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for MultiSignature {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for MultiSignature {
    fn verify_secure_generic<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        if self.multi_pk.pk_map.len() > MAX_SIGNER_IN_MULTISIG {
            return Err(SuiError::InvalidSignature {
                error: "Invalid number of public keys".to_string(),
            });
        }

        if <SuiAddress as From<MultiPublicKey>>::from(self.multi_pk.clone()) != author {
            return Err(SuiError::InvalidSignature {
                error: "Invalid address".to_string(),
            });
        }
        let mut weight_sum = 0;
        let message = bcs::to_bytes(&value).expect("Message serialization should not fail");

        for (sig, i) in self.sigs.iter().zip(&self.bitmap) {
            let pk_map =
                self.multi_pk
                    .pk_map
                    .get(i as usize)
                    .ok_or(SuiError::InvalidSignature {
                        error: "Invalid public keys index".to_string(),
                    })?;
            let res = match sig {
                CompressedSignature::Ed25519(s) => {
                    let pk = Ed25519PublicKey::from_bytes(pk_map.0.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(&message, s)
                }
                CompressedSignature::Secp256k1(s) => {
                    let pk = Secp256k1PublicKey::from_bytes(pk_map.0.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(&message, s)
                }
                CompressedSignature::Secp256r1(s) => {
                    let pk = Secp256r1PublicKey::from_bytes(pk_map.0.as_ref()).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid public key".to_string(),
                        }
                    })?;
                    pk.verify(&message, s)
                }
            };
            if res.is_ok() {
                weight_sum += pk_map.1 as u16;
            }
        }

        if weight_sum >= self.multi_pk.threshold {
            Ok(())
        } else {
            Err(SuiError::InvalidSignature {
                error: format!("Insufficient weight {:?}", weight_sum),
            })
        }
    }
}

impl MultiSignature {
    /// This combines a list of [enum Signature] `flag || signature || pk` to a multisignature.
    pub fn combine(full_sigs: Vec<Signature>, multi_pk: MultiPublicKey) -> Result<Self, SuiError> {
        if full_sigs.len() > multi_pk.pk_map.len()
            || multi_pk.pk_map.len() > MAX_SIGNER_IN_MULTISIG
            || full_sigs.is_empty()
            || multi_pk.pk_map.is_empty()
        {
            return Err(SuiError::InvalidSignature {
                error: "Invalid number of signatures".to_string(),
            });
        }
        let mut bitmap = RoaringBitmap::new();
        let mut sigs = Vec::new();
        full_sigs.iter().for_each(|s| {
            // TODO(joyqvq): Remove unwrap here.
            bitmap.insert(multi_pk.get_index(s.to_public_key().unwrap()).unwrap());
            sigs.push(s.to_compressed().unwrap());
        });

        Ok(MultiSignature {
            sigs,
            bitmap,
            multi_pk,
            bytes: OnceCell::new(),
        })
    }
}

/// The struct that contains the public key used for authenticating a multisig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MultiPublicKey {
    /// A list of public key and its corresponding weight.
    pk_map: Vec<(PublicKey, WeightUnit)>,
    /// If the total weight of the public keys corresponding to verified signatures is larger than threshold, the multisig is verified.
    threshold: ThresholdUnit,
}

impl MultiPublicKey {
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
        {
            return Err(SuiError::InvalidSignature {
                error: "Invalid number of public keys".to_string(),
            });
        }
        Ok(MultiPublicKey {
            pk_map: pks.into_iter().zip(weights.into_iter()).collect(),
            threshold,
        })
    }

    pub fn get_index(&self, pk: PublicKey) -> Option<u32> {
        self.pk_map.iter().position(|x| x.0 == pk).map(|x| x as u32)
    }

    pub fn threshold(&self) -> &ThresholdUnit {
        &self.threshold
    }

    pub fn pubkeys(&self) -> &Vec<(PublicKey, WeightUnit)> {
        &self.pk_map
    }
}

/// This ports the wrapper trait to the verify_secure defined on [enum Signature] (single signature).
impl AuthenticatorTrait for Signature {
    fn verify_secure_generic<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        self.verify_secure(value, author)
    }
}
