// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::crypto::PublicKey;
use crate::crypto::Secp256r1SuiSignature;
use crate::crypto::SuiSignatureInner;
use crate::signature_verification::VerifiedDigestCache;
use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{Signature, SignatureScheme, SuiSignature},
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
use passkey_types::webauthn::{ClientDataType, CollectedClientData};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

#[cfg(test)]
#[path = "unit_tests/passkey_session_authenticator_test.rs"]
mod passkey_session_authenticator_test;

/// An passkey session authenticator with parsed fields. See field defition below. Can be initialized from [struct RawPasskeySessionAuthenticator].
#[derive(Debug, Clone, JsonSchema)]
pub struct PasskeySessionAuthenticator {
    /// `authenticatorData` is a bytearray that encodes
    /// [Authenticator Data](https://www.w3.org/TR/webauthn-2/#sctn-authenticator-data)
    /// structure returned by the authenticator attestation
    /// response as is.
    authenticator_data: Vec<u8>,

    /// `clientDataJSON` contains a JSON-compatible
    /// UTF-8 encoded string of the client data which
    /// is passed to the authenticator by the client
    /// during the authentication request (see [CollectedClientData](https://www.w3.org/TR/webauthn-2/#dictdef-collectedclientdata))
    client_data_json: String,

    /// Normalized r1 signature returned by passkey. This signature commits to ephemral public key and max epoch.
    /// Initialized from `passkey_signature` in `RawPasskeySessionAuthenticator`.
    #[serde(skip)]
    passkey_signature: Secp256r1Signature,

    /// Compact r1 public key of the passkey.
    /// Initialized from `passkey_signature` in `RawPasskeySessionAuthenticator`.
    #[serde(skip)]
    passkey_pk: Secp256r1PublicKey,

    /// Ephemeral signature that commits to intent message of tx_data.
    ephemeral_signature: Signature,

    /// challenge field parsed from clientDataJSON. This should be `eph_flag || eph_pk || max_epoch`.
    parsed_challenge: Vec<u8>,

    /// Maximum epoch that the ephemeral signature is valid for.
    max_epoch: EpochId,

    /// Initialization of bytes for passkey in serialized form.
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// An raw passkey session authenticator struct used during deserialization. Can be converted to [struct RawPasskeySessionAuthenticator].
#[derive(Serialize, Deserialize, Debug)]
pub struct RawPasskeySessionAuthenticator {
    pub authenticator_data: Vec<u8>,
    pub client_data_json: String,
    pub passkey_signature: Signature,
    pub max_epoch: EpochId,
    pub ephemeral_signature: Signature,
}

/// Convert [struct RawPasskeySessionAuthenticator] to [struct RawPasskeySessionAuthenticator] with validations.
impl TryFrom<RawPasskeySessionAuthenticator> for PasskeySessionAuthenticator {
    type Error = SuiError;

    fn try_from(raw: RawPasskeySessionAuthenticator) -> Result<Self, Self::Error> {
        let client_data_json_parsed: CollectedClientData =
            serde_json::from_str(&raw.client_data_json).map_err(|_| {
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

        if raw.passkey_signature.scheme() != SignatureScheme::Secp256r1 {
            return Err(SuiError::InvalidSignature {
                error: "Invalid signature scheme".to_string(),
            });
        };

        let passkey_pk = Secp256r1PublicKey::from_bytes(raw.passkey_signature.public_key_bytes())
            .map_err(|_| SuiError::InvalidSignature {
            error: "Invalid r1 pk".to_string(),
        })?;

        let passkey_signature = Secp256r1Signature::from_bytes(
            raw.passkey_signature.signature_bytes(),
        )
        .map_err(|_| SuiError::InvalidSignature {
            error: "Invalid r1 sig".to_string(),
        })?;

        Ok(PasskeySessionAuthenticator {
            authenticator_data: raw.authenticator_data,
            client_data_json: raw.client_data_json,
            passkey_signature,
            passkey_pk,
            ephemeral_signature: raw.ephemeral_signature,
            parsed_challenge,
            max_epoch: raw.max_epoch,
            bytes: OnceCell::new(),
        })
    }
}

impl<'de> Deserialize<'de> for PasskeySessionAuthenticator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let serializable = RawPasskeySessionAuthenticator::deserialize(deserializer)?;
        serializable
            .try_into()
            .map_err(|e: SuiError| Error::custom(e.to_string()))
    }
}

impl Serialize for PasskeySessionAuthenticator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::with_capacity(Secp256r1SuiSignature::LENGTH);
        bytes.push(SignatureScheme::Secp256r1.flag());
        bytes.extend_from_slice(self.passkey_signature.as_ref());
        bytes.extend_from_slice(self.passkey_pk.as_ref());

        let raw = RawPasskeySessionAuthenticator {
            authenticator_data: self.authenticator_data.clone(),
            client_data_json: self.client_data_json.clone(),
            passkey_signature: Signature::Secp256r1SuiSignature(
                Secp256r1SuiSignature::from_bytes(&bytes).unwrap(), // This is safe because we just created the valid bytes.
            ),
            max_epoch: self.max_epoch,
            ephemeral_signature: self.ephemeral_signature.clone(),
        };
        raw.serialize(serializer)
    }
}
impl PasskeySessionAuthenticator {
    /// Returns the public key of the passkey authenticator.
    pub fn get_pk(&self) -> SuiResult<PublicKey> {
        Ok(PublicKey::PasskeySession((&self.passkey_pk).into()))
    }
}

/// Necessary trait for [struct SenderSignedData].
impl PartialEq for PasskeySessionAuthenticator {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

/// Necessary trait for [struct SenderSignedData].
impl Eq for PasskeySessionAuthenticator {}

/// Necessary trait for [struct SenderSignedData].
impl Hash for PasskeySessionAuthenticator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AuthenticatorTrait for PasskeySessionAuthenticator {
    fn verify_user_authenticator_epoch(
        &self,
        epoch: EpochId,
        max_epoch_upper_bound_delta: Option<u64>,
    ) -> SuiResult {
        // the checks here ensure that `current_epoch + passkey_session_max_epoch_upper_bound_delta >= self.max_epoch >= current_epoch`.
        // 1. if the config for upper bound is set, ensure that the max epoch in signature is not larger than epoch + upper_bound.
        if let Some(delta) = max_epoch_upper_bound_delta {
            let max_epoch_upper_bound =
                epoch.checked_add(delta).ok_or(SuiError::InvalidSignature {
                    error: "Max epoch upper bound delta overflow".to_string(),
                })?;
            if self.max_epoch > max_epoch_upper_bound {
                return Err(SuiError::InvalidSignature {
                    error: format!(
                        "Passkey session max epoch too large {}, current epoch {}, max accepted: {}",
                        self.max_epoch,
                        epoch,
                        max_epoch_upper_bound
                    ),
                });
            }
        }

        // 2. ensure that max epoch in signature is greater than the current epoch.
        if epoch > self.max_epoch {
            return Err(SuiError::InvalidSignature {
                error: format!(
                    "Passkey session expired at epoch {}, current epoch {}",
                    self.max_epoch, epoch
                ),
            });
        }
        Ok(())
    }

    /// Verify an intent message of a transaction with a passkey session authenticator.
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
        // Check if the challenge field is consistent with the ephemeral public key registered and its max epoch.
        let mut expected_register_msg = vec![self.ephemeral_signature.scheme().flag()];
        expected_register_msg.extend_from_slice(self.ephemeral_signature.public_key_bytes());
        expected_register_msg.extend_from_slice(&self.max_epoch.to_be_bytes());

        if self.parsed_challenge != expected_register_msg {
            return Err(SuiError::InvalidSignature {
                error: "Invalid parsed challenge".to_string(),
            });
        };

        // Check if author is derived from the public key.
        if author != SuiAddress::from(&self.get_pk()?) {
            return Err(SuiError::InvalidSignature {
                error: "Invalid author".to_string(),
            });
        };

        // Check if the ephemeral signature verifies against the transaction blake2b_hash(intent_message).
        self.ephemeral_signature
            .verify_secure(
                intent_msg,
                author,
                SignatureScheme::PasskeySessionAuthenticator,
            )
            .map_err(|_| SuiError::InvalidSignature {
                error: "Fails to verify ephemeral sig".to_string(),
            })?;

        // Construct msg = authenticator_data || sha256(client_data_json).
        let mut message = self.authenticator_data.clone();
        let client_data_hash = Sha256::digest(self.client_data_json.as_bytes()).digest;
        message.extend_from_slice(&client_data_hash);

        // Verify the passkey signature against pk and message.
        self.passkey_pk
            .verify(&message, &self.passkey_signature)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Fails to verify register sig".to_string(),
            })
    }
}

impl ToFromBytes for PasskeySessionAuthenticator {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        // The first byte matches the flag of PasskeySessionAuthenticator.
        if bytes.first().ok_or(FastCryptoError::InvalidInput)?
            != &SignatureScheme::PasskeySessionAuthenticator.flag()
        {
            return Err(FastCryptoError::InvalidInput);
        }
        let passkey: PasskeySessionAuthenticator =
            bcs::from_bytes(&bytes[1..]).map_err(|_| FastCryptoError::InvalidSignature)?;

        Ok(passkey)
    }
}

impl AsRef<[u8]> for PasskeySessionAuthenticator {
    fn as_ref(&self) -> &[u8] {
        self.bytes
            .get_or_try_init::<_, eyre::Report>(|| {
                let as_bytes = bcs::to_bytes(self).expect("BCS serialization should not fail");
                let mut bytes = Vec::with_capacity(1 + as_bytes.len());
                bytes.push(SignatureScheme::PasskeySessionAuthenticator.flag());
                bytes.extend_from_slice(as_bytes.as_slice());
                Ok(bytes)
            })
            .expect("OnceCell invariant violated")
    }
}
