// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    base_types::{EpochId, SuiAddress},
    crypto::{Signature, SignatureScheme, SuiSignature},
    error::{SuiError, SuiResult},
    signature::{AuthenticatorTrait, VerifyParams},
    zk_login_util::{AddressParams, DEFAULT_WHITELIST},
};
use fastcrypto::rsa::Encoding as OtherEncoding;
use fastcrypto::rsa::RSAPublicKey;
use fastcrypto::rsa::RSASignature;
use fastcrypto::{error::FastCryptoError, rsa::Base64UrlUnpadded, traits::ToFromBytes};
use fastcrypto_zkp::bn254::zk_login::verify_zk_login_proof_with_fixed_vk;
use fastcrypto_zkp::bn254::zk_login::{is_claim_supported, AuxInputs, PublicInputs, ZkLoginProof};
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
pub struct ZkLoginAuthenticator {
    proof: ZkLoginProof,
    public_inputs: PublicInputs,
    aux_inputs: AuxInputs,
    user_signature: Signature,

    #[serde(skip)]
    pub bytes: OnceCell<Vec<u8>>,
}

impl ZkLoginAuthenticator {
    /// Create a new [struct ZkLoginAuthenticator] with necessary fields.
    pub fn new(
        proof: ZkLoginProof,
        public_inputs: PublicInputs,
        aux_inputs: AuxInputs,
        user_signature: Signature,
    ) -> Self {
        Self {
            proof,
            public_inputs,
            aux_inputs,
            user_signature,
            bytes: OnceCell::new(),
        }
    }

    pub fn get_address_seed(&self) -> &str {
        self.aux_inputs.get_address_seed()
    }

    pub fn get_address_params(&self) -> AddressParams {
        AddressParams::new(
            self.aux_inputs.get_iss().to_string(),
            self.aux_inputs.get_claim_name().to_string(),
        )
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
        if epoch > self.aux_inputs.get_max_epoch() {
            return Err(SuiError::InvalidSignature {
                error: format!(
                    "ZKLogin expired at epoch {}",
                    self.aux_inputs.get_max_epoch()
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
    ) -> SuiResult
    where
        T: Serialize,
    {
        // Verify the author of the transaction is indeed computed from address seed,
        // iss and key claim name.
        if author != self.into() {
            return Err(SuiError::InvalidAddress);
        }
        let aux_inputs = &self.aux_inputs;

        if !is_claim_supported(aux_inputs.get_claim_name()) {
            return Err(SuiError::InvalidSignature {
                error: "Unsupported claim".to_string(),
            });
        }

        // Calculates the hash of all inputs equals to the one in public inputs.
        if aux_inputs
            .calculate_all_inputs_hash()
            .map_err(|_| SuiError::InvalidSignature {
                error: "Fail to caculate hash".to_string(),
            })?
            != self.public_inputs.get_all_inputs_hash()
        {
            return Err(SuiError::InvalidSignature {
                error: "Invalid all inputs hash".to_string(),
            });
        }

        // Parse JWT signature from aux inputs.
        let sig = RSASignature::from_bytes(&aux_inputs.get_jwt_signature().map_err(|_| {
            SuiError::InvalidSignature {
                error: "Invalid base64url".to_string(),
            }
        })?)
        .map_err(|_| SuiError::InvalidSignature {
            error: "Invalid JWT signature".to_string(),
        })?;

        let Some(selected) = aux_verify_data.oauth_provider_jwks.get(aux_inputs.get_kid()) else {
            return Err(SuiError::JWKRetrievalError);
        };

        // Verify the JWT signature against one of OAuth provider public keys in the bulletin.
        // Since more than one JWKs are available in the bulletin, iterate and find the one with
        // matching kid, iss and verify the signature against it.
        if !DEFAULT_WHITELIST
            .get(aux_inputs.get_iss())
            .unwrap()
            .contains(&aux_inputs.get_client_id())
        {
            return Err(SuiError::InvalidSignature {
                error: "Client id not in whitelist".to_string(),
            });
        }

        // TODO(joyqvq): cache RSAPublicKey and avoid parsing every time.
        let pk = RSAPublicKey::from_raw_components(
            &Base64UrlUnpadded::decode_vec(&selected.n).map_err(|_| {
                SuiError::InvalidSignature {
                    error: "Invalid OAuth provider pubkey n".to_string(),
                }
            })?,
            &Base64UrlUnpadded::decode_vec(&selected.e).map_err(|_| {
                SuiError::InvalidSignature {
                    error: "Invalid OAuth provider pubkey e".to_string(),
                }
            })?,
        )
        .map_err(|_| SuiError::InvalidSignature {
            error: "Invalid RSA raw components".to_string(),
        })?;

        pk.verify_prehash(&self.aux_inputs.get_jwt_hash(), &sig)
            .map_err(|_| SuiError::InvalidSignature {
                error: "JWT signature verify failed".to_string(),
            })?;

        // Ensure the ephemeral public key in the aux inputs matches the one in the
        // user signature.
        // TODO(joyqvq): possibly remove eph_pub_key from aux_inputs.
        if self.aux_inputs.get_eph_pub_key() != self.user_signature.public_key_bytes() {
            return Err(SuiError::InvalidSignature {
                error: "Invalid ephemeral public_key".to_string(),
            });
        }

        // Verify the user signature over the intent message of the transaction data.
        if self
            .user_signature
            .verify_secure(intent_msg, author, SignatureScheme::ZkLoginAuthenticator)
            .is_err()
        {
            return Err(SuiError::InvalidSignature {
                error: "User signature verify failed".to_string(),
            });
        }

        // Finally, verify the Groth16 proof against public inputs and proof points.
        // Verifying key is pinned in fastcrypto.
        match verify_zk_login_proof_with_fixed_vk(&self.proof, &self.public_inputs) {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(SuiError::InvalidSignature {
                error: "Groth16 proof verify failed".to_string(),
            }),
        }
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
        zk_login.aux_inputs.init()?;
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
