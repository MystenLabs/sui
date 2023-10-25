// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{DefaultHash, Signature, SignatureScheme, SuiSignature},
    digests::ZKLoginInputsDigest,
    error::{SuiError, SuiResult},
    signature::{AuthenticatorTrait, VerifyParams},
};
use fastcrypto::{error::FastCryptoError, traits::ToFromBytes};
use fastcrypto_zkp::bn254::zk_login::OIDCProvider;
use fastcrypto_zkp::bn254::{zk_login::ZkLoginInputs, zk_login_api::verify_zk_login};
use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
//#[cfg(any(test, feature = "test-utils"))]
#[cfg(test)]
#[path = "unit_tests/zk_login_authenticator_test.rs"]
mod zk_login_authenticator_test;

/// An zk login authenticator with all the necessary fields.
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZkLoginAuthenticator {
    inputs: ZkLoginInputs,
    max_epoch: EpochId,
    user_signature: Signature,
    #[serde(skip)]
    pub bytes: OnceCell<Vec<u8>>,
}

impl ZkLoginAuthenticator {
    pub fn hash_inputs(&self) -> ZKLoginInputsDigest {
        use fastcrypto::hash::HashFunction;
        let mut hasher = DefaultHash::default();
        hasher.update(bcs::to_bytes(&self.inputs).expect("serde should not fail"));
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

    pub fn get_max_epoch(&self) -> EpochId {
        self.max_epoch
    }

    pub fn get_address_seed(&self) -> &str {
        self.inputs.get_address_seed()
    }

    pub fn get_iss(&self) -> &str {
        self.inputs.get_iss()
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
    fn verify_user_authenticator_epoch(&self, epoch: EpochId) -> SuiResult {
        // Verify the max epoch in aux inputs is <= the current epoch of authority.
        if epoch > self.get_max_epoch() {
            return Err(SuiError::InvalidSignature {
                error: format!("ZKLogin expired at epoch {}", self.get_max_epoch()),
            });
        }
        Ok(())
    }

    /// This verifies the addresss derivation and ephemeral signature.
    /// It does not verify the zkLogin inputs (that includes the expensive zk proof verify).
    fn verify_uncached_checks<T>(
        &self,
        intent_msg: &IntentMessage<T>,
        author: SuiAddress,
        aux_verify_data: &VerifyParams,
    ) -> SuiResult
    where
        T: Serialize,
    {
        if aux_verify_data.verify_legacy_zklogin_address {
            if author != self.try_into()? && author != SuiAddress::legacy_try_from(self)? {
                return Err(SuiError::InvalidAddress);
            }
        } else if author != self.try_into()? {
            return Err(SuiError::InvalidAddress);
        }

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
        if self
            .user_signature
            .verify_secure(intent_msg, author, SignatureScheme::ZkLoginAuthenticator)
            .is_err()
        {
            return Err(SuiError::InvalidSignature {
                error: "Ephermal signature verify failed".to_string(),
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
    ) -> SuiResult
    where
        T: Serialize,
    {
        self.verify_uncached_checks(intent_msg, author, aux_verify_data)?;

        // Use flag || pk_bytes.
        let mut extended_pk_bytes = vec![self.user_signature.scheme().flag()];
        extended_pk_bytes.extend(self.user_signature.public_key_bytes());

        verify_zk_login(
            &self.inputs,
            self.max_epoch,
            &extended_pk_bytes,
            &aux_verify_data.oidc_provider_jwks,
            &aux_verify_data.zk_login_env,
        )
        .map_err(|e| SuiError::InvalidSignature {
            error: e.to_string(),
        })
    }
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
