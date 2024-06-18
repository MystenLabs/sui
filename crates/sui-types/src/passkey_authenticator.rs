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
use passkey::types::webauthn::{ClientDataType, CollectedClientData};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use shared_crypto::intent::{IntentMessage, INTENT_PREFIX_LENGTH};
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

#[cfg(test)]
#[path = "unit_tests/passkey_authenticator_test.rs"]
mod passkey_authenticator_test;

/// An passkey authenticator with all the necessary fields.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PasskeyAuthenticator {
    authenticator_data: Vec<u8>,
    client_data_json: Vec<u8>,
    user_signature: Signature,
    #[serde(skip)]
    signature: OnceCell<Secp256r1Signature>,
    #[serde(skip)]
    pk: OnceCell<Secp256r1PublicKey>,
    #[serde(skip)]
    parsed_challenge: Vec<u8>,
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RawPasskeyAuthenticator {
    pub authenticator_data: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub user_signature: Signature,
}

impl TryFrom<RawPasskeyAuthenticator> for PasskeyAuthenticator {
    type Error = SuiError;

    fn try_from(raw: RawPasskeyAuthenticator) -> Result<Self, Self::Error> {
        let client_data_json_parsed: CollectedClientData =
            serde_json::from_slice(&raw.client_data_json).map_err(|_| {
                SuiError::InvalidSignature {
                    error: "Invalid client data json".to_string(),
                }
            })?;

        if client_data_json_parsed.ty != ClientDataType::Get {
            return Err(SuiError::InvalidSignature {
                error: "Invalid client data type".to_string(),
            });
        };

        let parsed_challenge = Base64UrlUnpadded::decode_vec(&client_data_json_parsed.challenge)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid encoded challenge".to_string(),
            })?;

        if raw.user_signature.scheme() != SignatureScheme::Secp256r1 {
            return Err(SuiError::InvalidSignature {
                error: "Invalid signature scheme".to_string(),
            });
        };

        let pk = Secp256r1PublicKey::from_bytes(raw.user_signature.public_key_bytes()).map_err(
            |_| SuiError::InvalidSignature {
                error: "Invalid r1 pk".to_string(),
            },
        )?;
        let signature = Secp256r1Signature::from_bytes(raw.user_signature.signature_bytes())
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid r1 sig".to_string(),
            })?;

        Ok(PasskeyAuthenticator {
            authenticator_data: raw.authenticator_data,
            client_data_json: raw.client_data_json,
            user_signature: raw.user_signature,
            signature: signature.into(),
            pk: pk.into(),
            parsed_challenge,
            bytes: OnceCell::new(),
        })
    }
}

impl<'de> Deserialize<'de> for PasskeyAuthenticator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let serializable = RawPasskeyAuthenticator::deserialize(deserializer)?;
        serializable
            .try_into()
            .map_err(|e: SuiError| Error::custom(e.to_string()))
    }
}

impl PasskeyAuthenticator {
    pub fn new_for_testing(
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
        user_signature: Signature,
    ) -> Result<Self, SuiError> {
        let raw = RawPasskeyAuthenticator {
            authenticator_data,
            client_data_json,
            user_signature,
        };
        raw.try_into()
    }

    pub fn get_pk(&self) -> SuiResult<PublicKey> {
        PublicKey::try_from_bytes(
            SignatureScheme::PasskeyAuthenticator,
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
        // check parsed_challenge == intent || blake2b_hash(tx_data)
        let digest = to_signing_digest(intent_msg);
        if self.parsed_challenge != digest {
            return Err(SuiError::InvalidSignature {
                error: "Invalid challenge".to_string(),
            });
        };

        // construct msg = authenticator_data || sha256(client_data_json)
        let mut message = self.authenticator_data.clone();
        let client_data_hash = Sha256::digest(self.client_data_json.as_slice()).digest;
        message.extend_from_slice(&client_data_hash);

        if author != SuiAddress::from(&self.get_pk()?) {
            return Err(SuiError::InvalidSignature {
                error: "Invalid author".to_string(),
            });
        };

        let pk = self.pk.get().ok_or(SuiError::InvalidSignature {
            error: "Missing pk".to_string(),
        })?;
        let signature = self.signature.get().ok_or(SuiError::InvalidSignature {
            error: "Missing signature".to_string(),
        })?;

        pk.verify(&message, signature)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Fails to verify".to_string(),
            })
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
        let passkey: PasskeyAuthenticator =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;
        Ok(passkey)
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
/// Compute the digest that the signature committed over, that is: intent || hash(tx_data), total
/// of 3 + 32 = 35 bytes.
pub fn to_signing_digest<T: Serialize>(
    intent_msg: &IntentMessage<T>,
) -> [u8; INTENT_PREFIX_LENGTH + 32] {
    let mut extended = [0; INTENT_PREFIX_LENGTH + 32];
    extended[..INTENT_PREFIX_LENGTH].copy_from_slice(&intent_msg.intent.to_bytes());

    let mut hasher = DefaultHash::default();
    bcs::serialize_into(&mut hasher, &intent_msg.value)
        .expect("Message serialization should not fail");
    extended[INTENT_PREFIX_LENGTH..].copy_from_slice(&hasher.finalize().digest);
    extended
}
