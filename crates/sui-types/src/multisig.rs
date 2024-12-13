// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    crypto::{CompressedSignature, DefaultHash, SignatureScheme},
    digests::ZKLoginInputsDigest,
    passkey_authenticator::PasskeyAuthenticator,
    signature::{AuthenticatorTrait, GenericSignature, VerifyParams},
    signature_verification::VerifiedDigestCache,
    zk_login_authenticator::ZkLoginAuthenticator,
};
pub use enum_dispatch::enum_dispatch;
use fastcrypto::{
    ed25519::Ed25519PublicKey,
    encoding::{Base64, Encoding},
    error::FastCryptoError,
    hash::HashFunction,
    secp256k1::Secp256k1PublicKey,
    secp256r1::Secp256r1PublicKey,
    traits::{EncodeDecodeBase64, ToFromBytes, VerifyingKey},
};
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use shared_crypto::intent::IntentMessage;
use std::{
    hash::{Hash, Hasher},
    str::FromStr,
    sync::Arc,
};

use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::PublicKey,
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
    fn verify_user_authenticator_epoch(
        &self,
        epoch_id: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> Result<(), SuiError> {
        // If there is any zkLogin signatures, filter and check epoch for each.
        // TODO: call this on all sigs to avoid future lapses
        self.get_zklogin_sigs()?.iter().try_for_each(|s| {
            s.verify_user_authenticator_epoch(epoch_id, max_epoch_upper_bound_delta)
        })
    }

    fn verify_claims<T>(
        &self,
        value: &IntentMessage<T>,
        multisig_address: SuiAddress,
        verify_params: &VerifyParams,
        zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> Result<(), SuiError>
    where
        T: Serialize,
    {
        self.multisig_pk
            .validate()
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid multisig pubkey".to_string(),
            })?;

        if SuiAddress::from(&self.multisig_pk) != multisig_address {
            return Err(SuiError::InvalidSignature {
                error: "Invalid address derived from pks".to_string(),
            });
        }

        if !self.get_zklogin_sigs()?.is_empty() && !verify_params.accept_zklogin_in_multisig {
            return Err(SuiError::InvalidSignature {
                error: "zkLogin sig not supported inside multisig".to_string(),
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
            let (subsig_pubkey, weight) =
                self.multisig_pk
                    .pk_map
                    .get(i as usize)
                    .ok_or(SuiError::InvalidSignature {
                        error: "Invalid public keys index".to_string(),
                    })?;
            let res = match sig {
                CompressedSignature::Ed25519(s) => {
                    let pk =
                        Ed25519PublicKey::from_bytes(subsig_pubkey.as_ref()).map_err(|_| {
                            SuiError::InvalidSignature {
                                error: "Invalid ed25519 pk bytes".to_string(),
                            }
                        })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Invalid ed25519 signature bytes".to_string(),
                        })?,
                    )
                }
                CompressedSignature::Secp256k1(s) => {
                    let pk =
                        Secp256k1PublicKey::from_bytes(subsig_pubkey.as_ref()).map_err(|_| {
                            SuiError::InvalidSignature {
                                error: "Invalid k1 pk bytes".to_string(),
                            }
                        })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Invalid k1 signature bytes".to_string(),
                        })?,
                    )
                }
                CompressedSignature::Secp256r1(s) => {
                    let pk =
                        Secp256r1PublicKey::from_bytes(subsig_pubkey.as_ref()).map_err(|_| {
                            SuiError::InvalidSignature {
                                error: "Invalid r1 pk bytes".to_string(),
                            }
                        })?;
                    pk.verify(
                        &digest,
                        &s.try_into().map_err(|_| SuiError::InvalidSignature {
                            error: "Invalid r1 signature bytes".to_string(),
                        })?,
                    )
                }
                CompressedSignature::ZkLogin(z) => {
                    let authenticator = ZkLoginAuthenticator::from_bytes(&z.0).map_err(|_| {
                        SuiError::InvalidSignature {
                            error: "Invalid zklogin authenticator bytes".to_string(),
                        }
                    })?;
                    authenticator
                        .verify_claims(
                            value,
                            SuiAddress::from(subsig_pubkey),
                            verify_params,
                            zklogin_inputs_cache.clone(),
                        )
                        .map_err(|e| FastCryptoError::GeneralError(e.to_string()))
                }
                CompressedSignature::Passkey(bytes) => {
                    let authenticator =
                        PasskeyAuthenticator::from_bytes(&bytes.0).map_err(|_| {
                            SuiError::InvalidSignature {
                                error: "Invalid passkey authenticator bytes".to_string(),
                            }
                        })?;
                    authenticator
                        .verify_claims(
                            value,
                            SuiAddress::from(subsig_pubkey),
                            verify_params,
                            zklogin_inputs_cache.clone(),
                        )
                        .map_err(|e| FastCryptoError::GeneralError(e.to_string()))
                }
            };
            if res.is_ok() {
                weight_sum += *weight as u16;
            } else {
                return res.map_err(|e| SuiError::InvalidSignature {
                    error: format!(
                        "Invalid sig for pk={} address={:?} error={:?}",
                        subsig_pubkey.encode_base64(),
                        SuiAddress::from(subsig_pubkey),
                        e.to_string()
                    ),
                });
            }
        }
        if weight_sum >= self.multisig_pk.threshold {
            Ok(())
        } else {
            Err(SuiError::InvalidSignature {
                error: format!(
                    "Insufficient weight={:?} threshold={:?}",
                    weight_sum, self.multisig_pk.threshold
                ),
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
    /// Create MultiSig from its fields without validation
    pub fn insecure_new(
        sigs: Vec<CompressedSignature>,
        bitmap: BitmapUnit,
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
        full_sigs: Vec<GenericSignature>,
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

    pub fn init_and_validate(&mut self) -> Result<Self, FastCryptoError> {
        if self.sigs.len() > self.multisig_pk.pk_map.len()
            || self.sigs.is_empty()
            || self.bitmap > MAX_BITMAP_VALUE
        {
            return Err(FastCryptoError::InvalidInput);
        }
        self.multisig_pk.validate()?;
        Ok(self.to_owned())
    }

    pub fn get_pk(&self) -> &MultiSigPublicKey {
        &self.multisig_pk
    }

    pub fn get_sigs(&self) -> &[CompressedSignature] {
        &self.sigs
    }

    pub fn get_zklogin_sigs(&self) -> Result<Vec<ZkLoginAuthenticator>, SuiError> {
        let authenticator_as_bytes: Vec<_> = self
            .sigs
            .iter()
            .filter_map(|s| match s {
                CompressedSignature::ZkLogin(z) => Some(z),
                _ => None,
            })
            .collect();
        authenticator_as_bytes
            .iter()
            .map(|z| {
                ZkLoginAuthenticator::from_bytes(&z.0).map_err(|_| SuiError::InvalidSignature {
                    error: "Invalid zklogin authenticator bytes".to_string(),
                })
            })
            .collect()
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
        let mut multisig: MultiSig =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;
        multisig.init_and_validate()
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
    /// Construct MultiSigPublicKey without validation.
    pub fn insecure_new(pk_map: Vec<(PublicKey, WeightUnit)>, threshold: ThresholdUnit) -> Self {
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
            || pks
                .iter()
                .enumerate()
                .any(|(i, pk)| pks.iter().skip(i + 1).any(|other_pk| *pk == *other_pk))
        {
            return Err(SuiError::InvalidSignature {
                error: "Invalid multisig public key construction".to_string(),
            });
        }

        Ok(MultiSigPublicKey {
            pk_map: pks.into_iter().zip(weights).collect(),
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

    pub fn validate(&self) -> Result<MultiSigPublicKey, FastCryptoError> {
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
            || pk_map.iter().enumerate().any(|(i, (pk, _weight))| {
                pk_map
                    .iter()
                    .skip(i + 1)
                    .any(|(other_pk, _weight)| *pk == *other_pk)
            })
        {
            return Err(FastCryptoError::InvalidInput);
        }
        Ok(self.to_owned())
    }
}
