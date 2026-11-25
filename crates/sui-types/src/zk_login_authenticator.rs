// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::crypto::{PublicKey, ZkLoginPublicIdentifier};
use crate::signature_verification::VerifiedDigestCache;
use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{DefaultHash, Signature, SignatureScheme, SuiSignature},
    digests::ZKLoginInputsDigest,
    error::{SuiError, SuiErrorKind, SuiResult},
    signature::{AuthenticatorTrait, VerifyParams},
};
use fastcrypto::hash::HashFunction;
use fastcrypto::{error::FastCryptoError, traits::ToFromBytes};
use fastcrypto_zkp::bn254::zk_login::{OIDCProvider, VersionedZkLoginInputs, ZkLoginInputs};
use fastcrypto_zkp::bn254::zk_login_api::{verify_zk_login, verify_zk_login_extended};
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
#[cfg(test)]
#[path = "unit_tests/zk_login_authenticator_test.rs"]
mod zk_login_authenticator_test;

pub(crate) fn zklogin_mode_allows_v1(mode: u64) -> bool {
    matches!(mode, 0 | 1)
}

pub(crate) fn zklogin_mode_allows_v2(mode: u64) -> bool {
    mode == 1
}

/// An zk login authenticator with all the necessary fields.
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkLoginAuthenticator {
    #[serde(deserialize_with = "deserialize_v1_inputs")]
    pub inputs: ZkLoginInputs,
    max_epoch: EpochId,
    pub user_signature: Signature,
    #[serde(skip)]
    pub bytes: OnceCell<Vec<u8>>,
}

/// A zkLogin authenticator for the v2 circuit.
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkLoginAuthenticatorV2 {
    #[serde(deserialize_with = "deserialize_v2_inputs")]
    inputs: VersionedZkLoginInputs,
    max_epoch: EpochId,
    user_signature: Signature,
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// A helper struct that contains the necessary fields to calculate caching key.
/// If the verify_zk_login() api changes, additional fields must be added here
/// so the cache is not skipped.
#[derive(Serialize, Deserialize)]
struct ZkLoginCachingParams {
    inputs: ZkLoginInputs,
    max_epoch: EpochId,
    extended_pk_bytes: Vec<u8>,
    zklogin_circuit_mode: u64,
}

impl ZkLoginCachingParams {
    fn digest(&self) -> ZKLoginInputsDigest {
        let mut hasher = DefaultHash::default();
        bcs::serialize_into(&mut hasher, self).expect("serde should not fail");
        ZKLoginInputsDigest::new(hasher.finalize().into())
    }
}

impl ZkLoginAuthenticator {
    pub const FLAG: u8 = SignatureScheme::ZkLoginAuthenticator.flag();

    /// The caching key for zklogin signature, it is the hash of bcs bytes of
    /// ZkLoginInputs || max_epoch || flagged_pk_bytes || zklogin_circuit_mode. If any of these
    /// fields change, zklogin signature is re-verified without using the caching result.
    fn get_caching_params(&self, zklogin_circuit_mode: u64) -> ZkLoginCachingParams {
        let extended_pk_bytes = flagged_ephemeral_key(&self.user_signature);
        ZkLoginCachingParams {
            inputs: self.inputs.clone(),
            max_epoch: self.max_epoch,
            extended_pk_bytes,
            zklogin_circuit_mode,
        }
    }

    pub fn hash_inputs(&self, zklogin_circuit_mode: u64) -> ZKLoginInputsDigest {
        self.get_caching_params(zklogin_circuit_mode).digest()
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

impl ZkLoginAuthenticatorV2 {
    pub const FLAG: u8 = SignatureScheme::ZkLoginAuthenticatorV2.flag();

    pub fn new(
        mut inputs: VersionedZkLoginInputs,
        max_epoch: EpochId,
        user_signature: Signature,
    ) -> Result<Self, FastCryptoError> {
        inputs.init()?;
        Ok(Self {
            inputs,
            max_epoch,
            user_signature,
            bytes: OnceCell::new(),
        })
    }

    pub fn get_max_epoch(&self) -> EpochId {
        self.max_epoch
    }

    pub fn user_signature(&self) -> &Signature {
        &self.user_signature
    }

    pub fn get_iss(&self) -> &str {
        self.inputs.get_iss()
    }

    pub fn address_seed(&self) -> &[u8; 32] {
        self.inputs.get_address_seed()
    }

    pub fn get_pk(&self) -> PublicKey {
        PublicKey::ZkLoginV2(ZkLoginPublicIdentifier::new_from_padded_bytes(
            self.get_iss(),
            self.address_seed(),
        ))
    }
}

impl AuthenticatorTrait for ZkLoginAuthenticator {
    fn verify_user_authenticator_epoch(
        &self,
        epoch: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> SuiResult {
        verify_epoch(self.max_epoch, epoch, max_epoch_upper_bound_delta)
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
        if !zklogin_mode_allows_v1(aux_verify_data.zklogin_circuit_mode) {
            return Err(invalid_signature("zkLogin v1 is not enabled"));
        }

        // Always evaluate the unpadded address derivation.
        if author != SuiAddress::try_from_unpadded(&self.inputs)? {
            // If the verify_legacy_zklogin_address flag is set, also evaluate the padded address derivation.
            if !aux_verify_data.verify_legacy_zklogin_address
                || author != SuiAddress::try_from_padded(&self.inputs)?
            {
                return Err(SuiErrorKind::InvalidAddress.into());
            }
        }

        verify_ephemeral_signature(
            self.get_iss(),
            &self.user_signature,
            intent_msg,
            author,
            aux_verify_data,
        )?;

        let zklogin_circuit_mode = aux_verify_data.zklogin_circuit_mode;
        let caching_params = self.get_caching_params(zklogin_circuit_mode);
        let inputs_digest = caching_params.digest();
        if !zklogin_inputs_cache.is_cached(&inputs_digest) {
            verify_zk_login(
                &caching_params.inputs,
                caching_params.max_epoch,
                &caching_params.extended_pk_bytes,
                &aux_verify_data.oidc_provider_jwks,
                &aux_verify_data.zk_login_env,
            )
            .map_err(invalid_signature)?;
            zklogin_inputs_cache.cache_digest(inputs_digest);
        }
        Ok(())
    }
}

impl AuthenticatorTrait for ZkLoginAuthenticatorV2 {
    fn verify_user_authenticator_epoch(
        &self,
        epoch: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> SuiResult {
        verify_epoch(self.max_epoch, epoch, max_epoch_upper_bound_delta)
    }

    fn verify_claims<T: Serialize>(
        &self,
        intent_msg: &IntentMessage<T>,
        author: SuiAddress,
        params: &VerifyParams,
        zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> SuiResult {
        if !zklogin_mode_allows_v2(params.zklogin_circuit_mode) {
            return Err(invalid_signature("zkLogin v2 is not enabled"));
        }
        if author != SuiAddress::from(&self.get_pk()) {
            return Err(SuiErrorKind::InvalidAddress.into());
        }
        verify_ephemeral_signature(
            self.get_iss(),
            &self.user_signature,
            intent_msg,
            author,
            params,
        )?;

        let extended_pk_bytes = flagged_ephemeral_key(&self.user_signature);

        let mut hasher = DefaultHash::default();
        hasher.update(b"Sui zkLogin v2 proof");
        bcs::serialize_into(
            &mut hasher,
            &(
                &self.inputs,
                self.max_epoch,
                &extended_pk_bytes,
                params.zk_login_env,
            ),
        )
        .expect("zkLogin inputs have a BCS representation");
        let digest = ZKLoginInputsDigest::new(hasher.finalize().into());
        zklogin_inputs_cache.is_verified(
            digest,
            || {
                verify_zk_login_extended(
                    &self.inputs,
                    self.max_epoch,
                    &extended_pk_bytes,
                    &params.oidc_provider_jwks,
                    &params.zk_login_env,
                )
                .map_err(invalid_signature)
            },
            || Ok(()),
        )
    }
}

fn flagged_ephemeral_key(signature: &Signature) -> Vec<u8> {
    let mut key = vec![signature.scheme().flag()];
    key.extend_from_slice(signature.public_key_bytes());
    key
}

fn invalid_signature(error: impl ToString) -> SuiError {
    SuiErrorKind::InvalidSignature {
        error: error.to_string(),
    }
    .into()
}

fn verify_epoch(
    max_epoch: EpochId,
    epoch: EpochId,
    max_epoch_upper_bound_delta: Option<u64>,
) -> SuiResult {
    if let Some(delta) = max_epoch_upper_bound_delta {
        let max_epoch_upper_bound = epoch + delta;
        if max_epoch > max_epoch_upper_bound {
            return Err(invalid_signature(format!(
                "ZKLogin max epoch too large {max_epoch}, current epoch {epoch}, max accepted: {max_epoch_upper_bound}",
            )));
        }
    }
    if epoch > max_epoch {
        return Err(invalid_signature(format!(
            "ZKLogin expired at epoch {max_epoch}, current epoch {epoch}",
        )));
    }
    Ok(())
}

fn verify_ephemeral_signature<T: Serialize>(
    iss: &str,
    signature: &Signature,
    intent_msg: &IntentMessage<T>,
    author: SuiAddress,
    params: &VerifyParams,
) -> SuiResult {
    if !params.supported_providers.is_empty()
        && !params.supported_providers.contains(
            &OIDCProvider::from_iss(iss).map_err(|_| invalid_signature("Unknown provider"))?,
        )
    {
        return Err(invalid_signature(format!(
            "OIDC provider not supported: {iss}"
        )));
    }
    match signature.scheme() {
        SignatureScheme::ED25519 | SignatureScheme::Secp256k1 | SignatureScheme::Secp256r1 => {}
        scheme => {
            return Err(invalid_signature(format!(
                "zkLogin ephemeral signature scheme not supported: {scheme}"
            )));
        }
    }
    params.reject_compact_signature_key(signature)?;
    signature.verify_secure(intent_msg, author, SignatureScheme::ZkLoginAuthenticator)
}

fn deserialize_v1_inputs<'de, D>(deserializer: D) -> Result<ZkLoginInputs, D::Error>
where
    D: Deserializer<'de>,
{
    let mut inputs = ZkLoginInputs::deserialize(deserializer)?;
    inputs.init().map_err(serde::de::Error::custom)
}

fn deserialize_v2_inputs<'de, D>(deserializer: D) -> Result<VersionedZkLoginInputs, D::Error>
where
    D: Deserializer<'de>,
{
    let mut inputs = VersionedZkLoginInputs::deserialize(deserializer)?;
    inputs.init().map_err(serde::de::Error::custom)
}

impl ToFromBytes for ZkLoginAuthenticator {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        decode_zklogin_authenticator(bytes, Self::FLAG)
    }
}

impl ToFromBytes for ZkLoginAuthenticatorV2 {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        decode_zklogin_authenticator(bytes, Self::FLAG)
    }
}

impl AsRef<[u8]> for ZkLoginAuthenticator {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_init(|| encode_zklogin_authenticator(self, Self::FLAG))
    }
}

impl AsRef<[u8]> for ZkLoginAuthenticatorV2 {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_init(|| encode_zklogin_authenticator(self, Self::FLAG))
    }
}

impl PartialEq for ZkLoginAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl PartialEq for ZkLoginAuthenticatorV2 {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for ZkLoginAuthenticator {}

impl Eq for ZkLoginAuthenticatorV2 {}

impl Hash for ZkLoginAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl Hash for ZkLoginAuthenticatorV2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

fn decode_zklogin_authenticator<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    flag: u8,
) -> Result<T, FastCryptoError> {
    let payload = bytes
        .strip_prefix(&[flag])
        .ok_or(FastCryptoError::InvalidInput)?;
    bcs::from_bytes(payload).map_err(|_| FastCryptoError::InvalidSignature)
}

fn encode_zklogin_authenticator(authenticator: &impl Serialize, flag: u8) -> Vec<u8> {
    let mut bytes = vec![flag];
    bcs::serialize_into(&mut bytes, authenticator)
        .expect("zkLogin authenticator has a BCS representation");
    bytes
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
        if buf.is_empty() { &self.0[31..] } else { buf }
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
