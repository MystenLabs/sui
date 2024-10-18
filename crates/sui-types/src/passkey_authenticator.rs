// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::crypto::PublicKey;
use crate::crypto::Secp256r1SuiSignature;
use crate::crypto::SuiSignatureInner;
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
use passkey_types::webauthn::{ClientDataType, CollectedClientData};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;

#[cfg(test)]
#[path = "unit_tests/passkey_authenticator_test.rs"]
mod passkey_authenticator_test;

/// An passkey authenticator with parsed fields. See field defition below. Can be initialized from [struct RawPasskeyAuthenticator].
#[derive(Debug, Clone, JsonSchema)]
pub struct PasskeyAuthenticator {
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

    /// Normalized r1 signature returned by passkey.
    /// Initialized from `user_signature` in `RawPasskeyAuthenticator`.
    #[serde(skip)]
    signature: Secp256r1Signature,

    /// Compact r1 public key upon passkey creation.
    /// Initialized from `user_signature` in `RawPasskeyAuthenticator`.
    #[serde(skip)]
    pk: Secp256r1PublicKey,

    /// Decoded `client_data_json.challenge` which is expected to be the signing message
    /// `hash(Intent | bcs_message)`
    #[serde(skip)]
    challenge: [u8; DefaultHash::OUTPUT_SIZE],

    /// Initialization of bytes for passkey in serialized form.
    #[serde(skip)]
    bytes: OnceCell<Vec<u8>>,
}

/// An raw passkey authenticator struct used during deserialization. Can be converted to [struct PasskeyAuthenticator].
#[derive(Serialize, Deserialize, Debug)]
pub struct RawPasskeyAuthenticator {
    pub authenticator_data: Vec<u8>,
    pub client_data_json: String,
    pub user_signature: Signature,
}

/// Convert [struct RawPasskeyAuthenticator] to [struct PasskeyAuthenticator] with validations.
impl TryFrom<RawPasskeyAuthenticator> for PasskeyAuthenticator {
    type Error = SuiError;

    fn try_from(raw: RawPasskeyAuthenticator) -> Result<Self, Self::Error> {
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

        let challenge = Base64UrlUnpadded::decode_vec(&client_data_json_parsed.challenge)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Invalid encoded challenge".to_string(),
            })?
            .try_into()
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
            signature,
            pk,
            challenge,
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

impl Serialize for PasskeyAuthenticator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::with_capacity(Secp256r1SuiSignature::LENGTH);
        bytes.push(SignatureScheme::Secp256r1.flag());
        bytes.extend_from_slice(self.signature.as_ref());
        bytes.extend_from_slice(self.pk.as_ref());

        let raw = RawPasskeyAuthenticator {
            authenticator_data: self.authenticator_data.clone(),
            client_data_json: self.client_data_json.clone(),
            user_signature: Signature::Secp256r1SuiSignature(
                Secp256r1SuiSignature::from_bytes(&bytes).unwrap(),
            ),
        };
        raw.serialize(serializer)
    }
}
impl PasskeyAuthenticator {
    /// A constructor for [struct PasskeyAuthenticator] with custom
    /// defined fields. Used for testing.
    pub fn new_for_testing(
        authenticator_data: Vec<u8>,
        client_data_json: String,
        user_signature: Signature,
    ) -> Result<Self, SuiError> {
        let raw = RawPasskeyAuthenticator {
            authenticator_data,
            client_data_json,
            user_signature,
        };
        raw.try_into()
    }

    /// Returns the public key of the passkey authenticator.
    pub fn get_pk(&self) -> SuiResult<PublicKey> {
        Ok(PublicKey::Passkey((&self.pk).into()))
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
        // Check the intent and signing is consisted from what's parsed from client_data_json.challenge
        if self.challenge != to_signing_message(intent_msg) {
            return Err(SuiError::InvalidSignature {
                error: "Invalid challenge".to_string(),
            });
        };

        // Construct msg = authenticator_data || sha256(client_data_json).
        let mut message = self.authenticator_data.clone();
        let client_data_hash = Sha256::digest(self.client_data_json.as_bytes()).digest;
        message.extend_from_slice(&client_data_hash);

        // Check if author is derived from the public key.
        if author != SuiAddress::from(&self.get_pk()?) {
            return Err(SuiError::InvalidSignature {
                error: "Invalid author".to_string(),
            });
        };

        // Verify the signature against pk and message.
        self.pk
            .verify(&message, &self.signature)
            .map_err(|_| SuiError::InvalidSignature {
                error: "Fails to verify".to_string(),
            })
    }
}

impl ToFromBytes for PasskeyAuthenticator {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        // The first byte matches the flag of PasskeyAuthenticator.
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

/// Compute the signing digest that the signature committed over as `hash(intent || tx_data)`
pub fn to_signing_message<T: Serialize>(
    intent_msg: &IntentMessage<T>,
) -> [u8; DefaultHash::OUTPUT_SIZE] {
    let mut hasher = DefaultHash::default();
    bcs::serialize_into(&mut hasher, intent_msg).expect("Message serialization should not fail");
    hasher.finalize().digest
}
