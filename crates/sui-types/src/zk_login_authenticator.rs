// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::crypto::PublicKey;
use crate::signature_verification::VerifiedDigestCache;
use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{DefaultHash, Signature, SignatureScheme, SuiSignature},
    digests::ZKLoginInputsDigest,
    error::{SuiError, SuiResult},
    signature::{AuthenticatorTrait, VerifyParams},
};
use fastcrypto::{error::FastCryptoError, traits::ToFromBytes};
use fastcrypto_zkp::bn254::zk_login::JwkId;
use fastcrypto_zkp::bn254::zk_login::{OIDCProvider, JWK};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use fastcrypto_zkp::bn254::{zk_login::ZkLoginInputs, zk_login_api::verify_zk_login};
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
#[cfg(test)]
#[path = "unit_tests/zk_login_authenticator_test.rs"]
mod zk_login_authenticator_test;

/// An zk login authenticator with all the necessary fields.
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkLoginAuthenticator {
    pub inputs: ZkLoginInputs,
    max_epoch: EpochId,
    user_signature: Signature,
    #[serde(skip)]
    pub bytes: OnceCell<Vec<u8>>,
}

/// A helper struct that contains the necessary fields to calculate caching key.
/// If the verify_zk_login() api changes, additional fields must be added here
/// so the cache is not skipped.
#[derive(Serialize, Deserialize)]
struct ZkLoginCachingParams {
    inputs: ZkLoginInputs,
    max_epoch: EpochId,
    extended_pk_bytes: Vec<u8>,
}

impl ZkLoginAuthenticator {
    /// The caching key for zklogin signature, it is the hash of bcs bytes of
    /// ZkLoginInputs || max_epoch || flagged_pk_bytes. If any of these fields
    /// change, zklogin signature is re-verified without using the caching result.
    fn get_caching_params(&self) -> ZkLoginCachingParams {
        let mut extended_pk_bytes = vec![self.user_signature.scheme().flag()];
        extended_pk_bytes.extend(self.user_signature.public_key_bytes());
        ZkLoginCachingParams {
            inputs: self.inputs.clone(),
            max_epoch: self.max_epoch,
            extended_pk_bytes,
        }
    }

    pub fn hash_inputs(&self) -> ZKLoginInputsDigest {
        use fastcrypto::hash::HashFunction;
        let mut hasher = DefaultHash::default();
        hasher.update(bcs::to_bytes(&self.get_caching_params()).expect("serde should not fail"));
        ZKLoginInputsDigest::new(hasher.finalize().into())
    }

    /// Create a new [struct ZkLoginAuthenticator] with necessary fields.
    pub fn new(inputs: ZkLoginInputs, max_epoch: EpochId, user_signature: Signature) -> Self {
        Self {
            inputs,
            max_epoch,
            user_signature,
            bytes: OnceCell::new(),
        }
    }

    pub fn get_pk(&self) -> SuiResult<PublicKey> {
        PublicKey::from_zklogin_inputs(&self.inputs)
    }

    pub fn get_iss(&self) -> &str {
        self.inputs.get_iss()
    }

    pub fn get_max_epoch(&self) -> EpochId {
        self.max_epoch
    }

    pub fn user_signature_mut_for_testing(&mut self) -> &mut Signature {
        &mut self.user_signature
    }
    pub fn max_epoch_mut_for_testing(&mut self) -> &mut EpochId {
        &mut self.max_epoch
    }
    pub fn zk_login_inputs_mut_for_testing(&mut self) -> &mut ZkLoginInputs {
        &mut self.inputs
    }
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for ZkLoginAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for ZkLoginAuthenticator {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for ZkLoginAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for ZkLoginAuthenticator {
    fn verify_user_authenticator_epoch(
        &self,
        epoch: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> SuiResult {
        // the checks here ensure that `current_epoch + max_epoch_upper_bound_delta >= self.max_epoch >= current_epoch`.
        // 1. if the config for upper bound is set, ensure that the max epoch in signature is not larger than epoch + upper_bound.
        if let Some(delta) = max_epoch_upper_bound_delta {
            let max_epoch_upper_bound = epoch + delta;
            if self.get_max_epoch() > max_epoch_upper_bound {
                return Err(SuiError::InvalidSignature {
                    error: format!(
                        "ZKLogin max epoch too large {}, current epoch {}, max accepted: {}",
                        self.get_max_epoch(),
                        epoch,
                        max_epoch_upper_bound
                    ),
                });
            }
        }
        // 2. ensure that max epoch in signature is greater than the current epoch.
        if epoch > self.get_max_epoch() {
            return Err(SuiError::InvalidSignature {
                error: format!(
                    "ZKLogin expired at epoch {}, current epoch {}",
                    self.get_max_epoch(),
                    epoch
                ),
            });
        }
        Ok(())
    }

    /// Verify an intent message of a transaction with an zk login authenticator.
    fn verify_claims<T>(
        &self,
        intent_msg: &IntentMessage<T>,
        author: SuiAddress,
        aux_verify_data: &VerifyParams,
        zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> SuiResult
    where
        T: Serialize,
    {
        // Always evaluate the unpadded address derivation.
        if author != SuiAddress::try_from_unpadded(&self.inputs)? {
            // If the verify_legacy_zklogin_address flag is set, also evaluate the padded address derivation.
            if !aux_verify_data.verify_legacy_zklogin_address
                || author != SuiAddress::try_from_padded(&self.inputs)?
            {
                return Err(SuiError::InvalidAddress);
            }
        }

        // Only when supported_providers list is not empty, we check if the provider is supported. Otherwise,
        // we just use the JWK map to check if its supported.
        if !aux_verify_data.supported_providers.is_empty()
            && !aux_verify_data.supported_providers.contains(
                &OIDCProvider::from_iss(self.inputs.get_iss()).map_err(|_| {
                    SuiError::InvalidSignature {
                        error: "Unknown provider".to_string(),
                    }
                })?,
            )
        {
            return Err(SuiError::InvalidSignature {
                error: format!("OIDC provider not supported: {}", self.inputs.get_iss()),
            });
        }

        // Verify the ephemeral signature over the intent message of the transaction data.
        self.user_signature.verify_secure(
            intent_msg,
            author,
            SignatureScheme::ZkLoginAuthenticator,
        )?;

        if zklogin_inputs_cache.is_cached(&self.hash_inputs()) {
            // If the zklogin inputs hits the cache, we don't need to verify the zklogin
            // again that contains the heavy computation.
            Ok(())
        } else {
            // if it is not cached, we verify the full zklogin inputs.
            // build extended_pk_bytes as flag || pk_bytes.
            let mut extended_pk_bytes = vec![self.user_signature.scheme().flag()];
            extended_pk_bytes.extend(self.user_signature.public_key_bytes());
            let res = verify_zklogin_inputs_wrapper(
                self.get_caching_params(),
                &aux_verify_data.oidc_provider_jwks,
                &aux_verify_data.zk_login_env,
            )
            .map_err(|e| SuiError::InvalidSignature {
                error: e.to_string(),
            });
            match res {
                Ok(_) => {
                    // If it's verified ok, we cache the digest.
                    zklogin_inputs_cache.cache_digest(self.hash_inputs());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    }
}

fn verify_zklogin_inputs_wrapper(
    params: ZkLoginCachingParams,
    all_jwk: &im::HashMap<JwkId, JWK>,
    env: &ZkLoginEnv,
) -> SuiResult<()> {
    verify_zk_login(
        &params.inputs,
        params.max_epoch,
        &params.extended_pk_bytes,
        all_jwk,
        env,
    )
    .map_err(|e| SuiError::InvalidSignature {
        error: e.to_string(),
    })
}

impl ToFromBytes for ZkLoginAuthenticator {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        // The first byte matches the flag of MultiSig.
        if bytes.first().ok_or(FastCryptoError::InvalidInput)?
            != &SignatureScheme::ZkLoginAuthenticator.flag()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        let mut zk_login: ZkLoginAuthenticator =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;
        zk_login.inputs.init()?;
        Ok(zk_login)
    }
}

impl AsRef<[u8]> for ZkLoginAuthenticator {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let as_bytes = bcs::to_bytes(self).expect("BCS serialization should not fail");
                let mut bytes = Vec::with_capacity(1 + as_bytes.len());
                bytes.push(SignatureScheme::ZkLoginAuthenticator.flag());
                bytes.extend_from_slice(as_bytes.as_slice());
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}

#[derive(Debug, Clone)]
pub struct AddressSeed([u8; 32]);

impl AddressSeed {
    pub fn unpadded(&self) -> &[u8] {
        let mut buf = self.0.as_slice();

        while !buf.is_empty() && buf[0] == 0 {
            buf = &buf[1..];
        }

        // If the value is '0' then just return a slice of length 1 of the final byte
        if buf.is_empty() {
            &self.0[31..]
        } else {
            buf
        }
    }

    pub fn padded(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for AddressSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let big_int = num_bigint::BigUint::from_bytes_be(&self.0);
        let radix10 = big_int.to_str_radix(10);
        f.write_str(&radix10)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AddressSeedParseError {
    #[error("unable to parse radix10 encoded value `{0}`")]
    Parse(#[from] num_bigint::ParseBigIntError),
    #[error("larger than 32 bytes")]
    TooBig,
}

impl std::str::FromStr for AddressSeed {
    type Err = AddressSeedParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let big_int = <num_bigint::BigUint as num_traits::Num>::from_str_radix(s, 10)?;
        let be_bytes = big_int.to_bytes_be();
        let len = be_bytes.len();
        let mut buf = [0; 32];

        if len > 32 {
            return Err(AddressSeedParseError::TooBig);
        }

        buf[32 - len..].copy_from_slice(&be_bytes);
        Ok(Self(buf))
    }
}

// AddressSeed's serialized format is as a radix10 encoded string
impl Serialize for AddressSeed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AddressSeed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = std::borrow::Cow::<'de, str>::deserialize(deserializer)?;
        std::str::FromStr::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::AddressSeed;
    use num_bigint::BigUint;
    use proptest::prelude::*;

    #[test]
    fn unpadded_slice() {
        let seed = AddressSeed([0; 32]);
        let zero: [u8; 1] = [0];
        assert_eq!(seed.unpadded(), zero.as_slice());

        let mut seed = AddressSeed([1; 32]);
        seed.0[0] = 0;
        assert_eq!(seed.unpadded(), [1; 31].as_slice());
    }

    proptest! {
        #[test]
        fn dont_crash_on_large_inputs(
            bytes in proptest::collection::vec(any::<u8>(), 33..1024)
        ) {
            let big_int = BigUint::from_bytes_be(&bytes);
            let radix10 = big_int.to_str_radix(10);

            // doesn't crash
            let _ = AddressSeed::from_str(&radix10);
        }

        #[test]
        fn valid_address_seeds(
            bytes in proptest::collection::vec(any::<u8>(), 1..=32)
        ) {
            let big_int = BigUint::from_bytes_be(&bytes);
            let radix10 = big_int.to_str_radix(10);

            let seed = AddressSeed::from_str(&radix10).unwrap();
            assert_eq!(radix10, seed.to_string());
            // Ensure unpadded doesn't crash
            seed.unpadded();
        }
    }
}
