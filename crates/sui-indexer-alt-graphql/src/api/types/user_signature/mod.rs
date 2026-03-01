// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod multisig;
pub(crate) mod passkey;
pub(crate) mod zklogin;

use async_graphql::{Object, SimpleObject, Union};
use sui_types::crypto::{SignatureScheme as NativeSignatureScheme, SuiSignature};
use sui_types::multisig::MultiSig;
use sui_types::signature::GenericSignature;

use crate::api::scalars::base64::Base64;

use self::multisig::MultisigSignature;
use self::passkey::PasskeySignature;
use self::zklogin::ZkLoginSignature;

/// A user signature for a transaction.
#[derive(Clone)]
pub(crate) struct UserSignature {
    pub(crate) native: GenericSignature,
}

#[Object]
impl UserSignature {
    /// The signature bytes, Base64-encoded.
    /// For simple signatures: flag || signature || pubkey
    /// For complex signatures: flag || bcs_serialized_struct
    async fn signature_bytes(&self) -> Option<Base64> {
        Some(Base64(self.native.as_ref().to_vec()))
    }

    /// The structured signature details, parsed by scheme.
    async fn scheme(&self) -> Option<SignatureScheme> {
        match &self.native {
            GenericSignature::Signature(s) => Some(simple_signature_to_scheme(s)),
            GenericSignature::MultiSig(m) => Some(SignatureScheme::Multisig(m.into())),
            GenericSignature::MultiSigLegacy(m) => MultiSig::try_from(m.clone())
                .ok()
                .map(|ms| SignatureScheme::Multisig((&ms).into())),
            GenericSignature::ZkLoginAuthenticator(z) => Some(SignatureScheme::ZkLogin(z.into())),
            GenericSignature::PasskeyAuthenticator(p) => Some(SignatureScheme::Passkey(p.into())),
        }
    }
}

impl UserSignature {
    pub(crate) fn from_generic_signature(signature: GenericSignature) -> Self {
        Self { native: signature }
    }
}

/// The structured details of a signature, varying by scheme.
#[derive(Union, Clone)]
pub(crate) enum SignatureScheme {
    Ed25519(Ed25519Signature),
    Secp256k1(Secp256k1Signature),
    Secp256r1(Secp256r1Signature),
    Multisig(MultisigSignature),
    ZkLogin(ZkLoginSignature),
    Passkey(PasskeySignature),
}

/// An Ed25519 signature.
#[derive(SimpleObject, Clone)]
pub(crate) struct Ed25519Signature {
    /// The raw signature bytes.
    signature: Base64,
    /// The public key bytes.
    public_key: Base64,
}

/// A Secp256k1 signature.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256k1Signature {
    /// The raw signature bytes.
    signature: Base64,
    /// The public key bytes.
    public_key: Base64,
}

/// A Secp256r1 signature.
#[derive(SimpleObject, Clone)]
pub(crate) struct Secp256r1Signature {
    /// The raw signature bytes.
    signature: Base64,
    /// The public key bytes.
    public_key: Base64,
}

/// Converts a native `Signature` (ed25519/secp256k1/secp256r1) into the corresponding
/// `SignatureScheme` union variant.
pub(crate) fn simple_signature_to_scheme(sig: &sui_types::crypto::Signature) -> SignatureScheme {
    let signature = Base64(sig.signature_bytes().to_vec());
    let public_key = Base64(sig.public_key_bytes().to_vec());

    match sig.scheme() {
        NativeSignatureScheme::ED25519 => SignatureScheme::Ed25519(Ed25519Signature {
            signature,
            public_key,
        }),
        NativeSignatureScheme::Secp256k1 => SignatureScheme::Secp256k1(Secp256k1Signature {
            signature,
            public_key,
        }),
        NativeSignatureScheme::Secp256r1 => SignatureScheme::Secp256r1(Secp256r1Signature {
            signature,
            public_key,
        }),
        _ => unreachable!("Signature enum only contains ed25519, secp256k1, secp256r1"),
    }
}
