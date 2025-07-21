// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Enum, Object};
use sui_types::{
    crypto::{SignatureScheme as NativeSignatureScheme, SuiSignature},
    signature::GenericSignature,
};

use crate::api::scalars::base64::Base64;

/// A user signature for a transaction.
#[derive(Clone)]
pub(crate) struct UserSignature {
    pub(crate) native: GenericSignature,
}

/// Flag used to disambiguate the signature schemes supported by Sui.
///
/// The enum values match their BCS serialized values when serialized as a u8.
/// See https://mystenlabs.github.io/sui-rust-sdk/sui_sdk_types/enum.SignatureScheme.html
/// for more information about signature schemes.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum SignatureScheme {
    /// Ed25519 signature scheme.
    Ed25519,
    /// Secp256k1 signature scheme.
    Secp256k1,
    /// Secp256r1 signature scheme.
    Secp256r1,
    /// BLS12-381 signature scheme.
    Bls12381,
    /// Multi-signature scheme.
    MultiSig,
    /// ZkLogin authenticator scheme.
    ZkLogin,
    /// Passkey authenticator scheme.
    Passkey,
}

#[Object]
impl UserSignature {
    /// The signature scheme used for this signature.
    async fn scheme(&self) -> Option<SignatureScheme> {
        Some(Self::extract_scheme(&self.native))
    }

    /// The signature bytes, Base64-encoded.
    /// For simple signatures: flag || signature || pubkey
    /// For complex signatures: flag || bcs_serialized_struct
    async fn signature_bytes(&self) -> Option<Base64> {
        Some(Base64(self.native.as_ref().to_vec()))
    }
}

// TODO(DVX-786): Support signature details.
impl UserSignature {
    pub(crate) fn from_generic_signature(signature: GenericSignature) -> Self {
        Self { native: signature }
    }

    /// Extract the signature scheme from a GenericSignature.
    /// This follows the same logic as the gRPC implementation.
    fn extract_scheme(sig: &GenericSignature) -> SignatureScheme {
        match sig {
            GenericSignature::Signature(s) => s.scheme().into(),
            GenericSignature::MultiSig(_) | GenericSignature::MultiSigLegacy(_) => {
                SignatureScheme::MultiSig
            }
            GenericSignature::ZkLoginAuthenticator(_) => SignatureScheme::ZkLogin,
            GenericSignature::PasskeyAuthenticator(_) => SignatureScheme::Passkey,
        }
    }
}

impl From<NativeSignatureScheme> for SignatureScheme {
    fn from(scheme: NativeSignatureScheme) -> Self {
        match scheme {
            NativeSignatureScheme::ED25519 => SignatureScheme::Ed25519,
            NativeSignatureScheme::Secp256k1 => SignatureScheme::Secp256k1,
            NativeSignatureScheme::Secp256r1 => SignatureScheme::Secp256r1,
            NativeSignatureScheme::BLS12381 => SignatureScheme::Bls12381,
            NativeSignatureScheme::MultiSig => SignatureScheme::MultiSig,
            NativeSignatureScheme::ZkLoginAuthenticator => SignatureScheme::ZkLogin,
            NativeSignatureScheme::PasskeyAuthenticator => SignatureScheme::Passkey,
        }
    }
}
