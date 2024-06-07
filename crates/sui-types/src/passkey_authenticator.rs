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
use fastcrypto::hash::{HashFunction, Sha256};
use fastcrypto::rsa::{Base64UrlUnpadded, Encoding};
use fastcrypto::secp256r1::{Secp256r1PublicKey, Secp256r1Signature};
use fastcrypto::traits::VerifyingKey;
use fastcrypto::{error::FastCryptoError, traits::ToFromBytes};
use once_cell::sync::OnceCell;
use passkey::types::webauthn::CollectedClientData;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

#[cfg(test)]
#[path = "unit_tests/passkey_authenticator_test.rs"]
mod passkey_authenticator_test;

/// An passkey authenticator with all the necessary fields.
#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyAuthenticator {
    authenticator_data: Vec<u8>, // should size limit be enforced?
    client_data_json: Vec<u8>,   // should size limit be enforced?
    user_signature: Signature,
    #[serde(skip)]
    pub bytes: OnceCell<Vec<u8>>,
}

impl PasskeyAuthenticator {
    // test only
    pub fn new(
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
        user_signature: Signature,
    ) -> Self {
        Self {
            authenticator_data,
            client_data_json,
            user_signature,
            bytes: OnceCell::new(),
        }
    }

    pub fn get_pk(&self) -> SuiResult<PublicKey> {
        PublicKey::try_from_bytes(
            SignatureScheme::Secp256r1,
            self.user_signature.public_key_bytes(),
        )
        .map_err(|_| SuiError::InvalidAuthenticator)
    }
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for PasskeyAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for PasskeyAuthenticator {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for PasskeyAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for PasskeyAuthenticator {
    fn verify_user_authenticator_epoch(
        &self,
        _epoch: EpochId,
        _max_epoch_upper_bound_delta: Option<u64>,
    ) -> SuiResult {
        Ok(())
    }

    /// Verify an intent message of a transaction with an passkey authenticator.
    fn verify_claims<T>(
        &self,
        intent_msg: &IntentMessage<T>,
        author: SuiAddress,
        _aux_verify_data: &VerifyParams,
        _zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,
    ) -> SuiResult
    where
        T: Serialize,
    {
        let client_data_json: CollectedClientData = serde_json::from_slice(&self.client_data_json)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Failed to parse client data".to_string(),
            })?;
        let parsed_challenge =
            Base64UrlUnpadded::decode_vec(&client_data_json.challenge).map_err(|e| {
                SuiError::InvalidSignature {
                    error: e.to_string(),
                }
            })?;
        let mut hasher = DefaultHash::default();
        hasher.update(&bcs::to_bytes(&intent_msg).expect("Message serialization should not fail"));
        let digest = hasher.finalize().digest;

        // check client_data_json.challenge == sha256(intent_msg(tx))
        if parsed_challenge != digest {
            return Err(SuiError::InvalidSignature {
                error: "invalid challenge".to_string(),
            });
        };

        // msg = authenticator_data || sha256(client_data_json)
        let mut message = self.authenticator_data.clone();
        let client_data_hash = Sha256::digest(self.client_data_json.as_slice()).digest;
        message.extend_from_slice(&client_data_hash);

        match self.user_signature.scheme() {
            SignatureScheme::Secp256r1 => {
                let pk = Secp256r1PublicKey::from_bytes(self.user_signature.public_key_bytes())
                    .map_err(|_| SuiError::InvalidSignature {
                        error: "Invalid r1 pk bytes".to_string(),
                    })?;
                if author
                    != SuiAddress::from(
                        &PublicKey::try_from_bytes(
                            SignatureScheme::Secp256r1,
                            self.user_signature.public_key_bytes(),
                        )
                        .unwrap(),
                    )
                {
                    return Err(SuiError::InvalidSignature {
                        error: "Invalid author".to_string(),
                    });
                };
                let sig = Secp256r1Signature::from_bytes(self.user_signature.signature_bytes())
                    .map_err(|_| SuiError::InvalidSignature {
                        error: "Invalid r1 signature bytes".to_string(),
                    })?;

                pk.verify(&message, &sig)
                    .map_err(|_| SuiError::InvalidSignature {
                        error: "verify failed".to_string(),
                    })
            }
            _ => Err(SuiError::InvalidSignature {
                error: "Invalid signature scheme for passkey".to_string(),
            }),
        }
    }
}

impl ToFromBytes for PasskeyAuthenticator {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        // The first byte matches the flag of MultiSig.
        if bytes.first().ok_or(FastCryptoError::InvalidInput)?
            != &SignatureScheme::PasskeyAuthenticator.flag()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        let zk_login: PasskeyAuthenticator =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;
        Ok(zk_login)
    }
}

impl AsRef<[u8]> for PasskeyAuthenticator {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let as_bytes = bcs::to_bytes(self).expect("BCS serialization should not fail");
                let mut bytes = Vec::with_capacity(1 + as_bytes.len());
                bytes.push(SignatureScheme::PasskeyAuthenticator.flag());
                bytes.extend_from_slice(as_bytes.as_slice());
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}
