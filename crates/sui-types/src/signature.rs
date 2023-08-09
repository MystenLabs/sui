// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::EpochId;
use crate::crypto::{SignatureScheme, SuiSignature};
use crate::multisig_legacy::MultiSigLegacy;
use crate::zk_login_authenticator::ZkLoginAuthenticator;
use crate::zk_login_util::OAuthProviderContent;
use crate::{base_types::SuiAddress, crypto::Signature, error::SuiResult, multisig::MultiSig};
pub use enum_dispatch::enum_dispatch;
use fastcrypto::{
    error::FastCryptoError,
    traits::{EncodeDecodeBase64, ToFromBytes},
};
use im::hashmap::HashMap as ImHashMap;
use schemars::JsonSchema;
use serde::Serialize;
use shared_crypto::intent::IntentMessage;
use std::hash::Hash;

#[derive(Default, Debug, Clone)]
pub struct VerifyParams {
    // map from kid => OauthProviderContent
    pub oauth_provider_jwks: ImHashMap<String, OAuthProviderContent>,
}

impl VerifyParams {
    pub fn new(oauth_provider_jwks: ImHashMap<String, OAuthProviderContent>) -> Self {
        Self {
            oauth_provider_jwks,
        }
    }
}

/// A lightweight trait that all members of [enum GenericSignature] implement.
#[enum_dispatch]
pub trait AuthenticatorTrait {
    fn verify_user_authenticator_epoch(&self, epoch: EpochId) -> SuiResult;

    fn verify_claims<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        aux_verify_data: &VerifyParams,
    ) -> SuiResult
    where
        T: Serialize;

    fn verify_authenticator<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        epoch: Option<EpochId>,
        aux_verify_data: &VerifyParams,
    ) -> SuiResult
    where
        T: Serialize,
    {
        if let Some(epoch) = epoch {
            self.verify_user_authenticator_epoch(epoch)?;
        }
        self.verify_claims(value, author, aux_verify_data)
    }
}

/// Due to the incompatibility of [enum Signature] (which dispatches a trait that
/// assumes signature and pubkey bytes for verification), here we add a wrapper
/// enum where member can just implement a lightweight [trait AuthenticatorTrait].
/// This way MultiSig (and future Authenticators) can implement its own `verify`.
#[enum_dispatch(AuthenticatorTrait)]
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Hash)]
pub enum GenericSignature {
    MultiSig,
    MultiSigLegacy,
    Signature,
    ZkLoginAuthenticator,
}

impl GenericSignature {
    pub fn is_zklogin(&self) -> bool {
        matches!(self, GenericSignature::ZkLoginAuthenticator(_))
    }

    pub fn is_upgraded_multisig(&self) -> bool {
        matches!(self, GenericSignature::MultiSig(_))
    }
}

/// GenericSignature encodes a single signature [enum Signature] as is `flag || signature || pubkey`.
/// It encodes [struct MultiSigLegacy] as the MultiSig flag (0x03) concat with the bcs serializedbytes
/// of [struct MultiSigLegacy] i.e. `flag || bcs_bytes(MultiSigLegacy)`.
/// [struct Multisig] is encodede as the MultiSig flag (0x03) concat with the bcs serializedbytes
/// of [struct Multisig] i.e. `flag || bcs_bytes(Multisig)`.
impl ToFromBytes for GenericSignature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        match SignatureScheme::from_flag_byte(
            bytes.first().ok_or(FastCryptoError::InputTooShort(0))?,
        ) {
            Ok(x) => match x {
                SignatureScheme::ED25519
                | SignatureScheme::Secp256k1
                | SignatureScheme::Secp256r1 => Ok(GenericSignature::Signature(
                    Signature::from_bytes(bytes).map_err(|_| FastCryptoError::InvalidSignature)?,
                )),
                SignatureScheme::MultiSig => match MultiSig::from_bytes(bytes) {
                    Ok(multisig) => Ok(GenericSignature::MultiSig(multisig)),
                    Err(_) => {
                        let multisig = MultiSigLegacy::from_bytes(bytes)?;
                        Ok(GenericSignature::MultiSigLegacy(multisig))
                    }
                },
                SignatureScheme::ZkLoginAuthenticator => {
                    let zk_login = ZkLoginAuthenticator::from_bytes(bytes)?;
                    Ok(GenericSignature::ZkLoginAuthenticator(zk_login))
                }
                _ => Err(FastCryptoError::InvalidInput),
            },
            Err(_) => Err(FastCryptoError::InvalidInput),
        }
    }
}

/// Trait useful to get the bytes reference for [enum GenericSignature].
impl AsRef<[u8]> for GenericSignature {
    fn as_ref(&self) -> &[u8] {
        match self {
            GenericSignature::MultiSig(s) => s.as_ref(),
            GenericSignature::MultiSigLegacy(s) => s.as_ref(),
            GenericSignature::Signature(s) => s.as_ref(),
            GenericSignature::ZkLoginAuthenticator(s) => s.as_ref(),
        }
    }
}

impl ::serde::Serialize for GenericSignature {
    fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            #[derive(serde::Serialize)]
            struct GenericSignature(String);
            GenericSignature(self.encode_base64()).serialize(serializer)
        } else {
            #[derive(serde::Serialize)]
            struct GenericSignature<'a>(&'a [u8]);
            GenericSignature(self.as_ref()).serialize(serializer)
        }
    }
}

impl<'de> ::serde::Deserialize<'de> for GenericSignature {
    fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        if deserializer.is_human_readable() {
            #[derive(serde::Deserialize)]
            struct GenericSignature(String);
            let s = GenericSignature::deserialize(deserializer)?;
            Self::decode_base64(&s.0).map_err(::serde::de::Error::custom)
        } else {
            #[derive(serde::Deserialize)]
            struct GenericSignature(Vec<u8>);

            let data = GenericSignature::deserialize(deserializer)?;
            Self::from_bytes(&data.0).map_err(|e| Error::custom(e.to_string()))
        }
    }
}

/// This ports the wrapper trait to the verify_secure defined on [enum Signature].
impl AuthenticatorTrait for Signature {
    fn verify_user_authenticator_epoch(&self, _: EpochId) -> SuiResult {
        Ok(())
    }

    fn verify_claims<T>(
        &self,
        value: &IntentMessage<T>,
        author: SuiAddress,
        _aux_verify_data: &VerifyParams,
    ) -> SuiResult
    where
        T: Serialize,
    {
        self.verify_secure(value, author, self.scheme())
    }
}
