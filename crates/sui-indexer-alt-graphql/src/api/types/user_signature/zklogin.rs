// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Object, SimpleObject};
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;

use crate::api::scalars::uint53::UInt53;

use super::multisig::ZkLoginPublicIdentifier;
use super::{SignatureScheme, simple_signature_to_scheme};

/// A zkLogin signature.
#[derive(Clone)]
pub(crate) struct ZkLoginSignature {
    pub(super) native: ZkLoginAuthenticator,
}

#[Object]
impl ZkLoginSignature {
    /// The maximum epoch for which this signature is valid.
    async fn max_epoch(&self) -> Option<UInt53> {
        Some(self.native.get_max_epoch().into())
    }

    /// The inner user signature (ed25519/secp256k1/secp256r1).
    async fn signature(&self) -> Option<SignatureScheme> {
        simple_signature_to_scheme(&self.native.user_signature)
    }

    /// The public identifier (issuer + address seed) for this zkLogin authenticator.
    async fn public_identifier(&self) -> Option<ZkLoginPublicIdentifier> {
        let sdk = sui_sdk_types::ZkLoginAuthenticator::try_from(self.native.clone()).ok()?;
        let id = sdk.inputs.public_identifier();
        Some(ZkLoginPublicIdentifier {
            iss: Some(id.iss().to_owned()),
            address_seed: Some(id.address_seed().to_string()),
        })
    }

    /// The zkLogin inputs including proof, claim details, and JWT header.
    async fn inputs(&self) -> Option<ZkLoginInputs> {
        let sdk = sui_sdk_types::ZkLoginAuthenticator::try_from(self.native.clone()).ok()?;
        let inputs = &sdk.inputs;
        let proof = inputs.proof_points();
        let claim = inputs.iss_base64_details();

        Some(ZkLoginInputs {
            proof_points: Some(ZkLoginProof {
                a: Some(CircomG1 {
                    e0: Some(proof.a.0[0].to_string()),
                    e1: Some(proof.a.0[1].to_string()),
                    e2: Some(proof.a.0[2].to_string()),
                }),
                b: Some(CircomG2 {
                    e00: Some(proof.b.0[0][0].to_string()),
                    e01: Some(proof.b.0[0][1].to_string()),
                    e10: Some(proof.b.0[1][0].to_string()),
                    e11: Some(proof.b.0[1][1].to_string()),
                    e20: Some(proof.b.0[2][0].to_string()),
                    e21: Some(proof.b.0[2][1].to_string()),
                }),
                c: Some(CircomG1 {
                    e0: Some(proof.c.0[0].to_string()),
                    e1: Some(proof.c.0[1].to_string()),
                    e2: Some(proof.c.0[2].to_string()),
                }),
            }),
            iss_base64_details: Some(ZkLoginClaim {
                value: Some(claim.value.clone()),
                index_mod_4: Some(claim.index_mod_4),
            }),
            header_base64: Some(inputs.header_base64().to_owned()),
            address_seed: Some(inputs.address_seed().to_string()),
        })
    }

    /// The JWK identifier used to verify the zkLogin proof.
    async fn jwk_id(&self) -> Option<ZkLoginJwkId> {
        let sdk = sui_sdk_types::ZkLoginAuthenticator::try_from(self.native.clone()).ok()?;
        let jwk = sdk.inputs.jwk_id();
        Some(ZkLoginJwkId {
            iss: Some(jwk.iss.clone()),
            kid: Some(jwk.kid.clone()),
        })
    }
}

/// The zkLogin inputs including proof, claim details, and JWT header.
#[derive(SimpleObject, Clone)]
pub(crate) struct ZkLoginInputs {
    /// The zero-knowledge proof points.
    proof_points: Option<ZkLoginProof>,
    /// The Base64-encoded issuer claim details.
    iss_base64_details: Option<ZkLoginClaim>,
    /// The Base64-encoded JWT header.
    header_base64: Option<String>,
    /// The address seed as a base10-encoded string.
    address_seed: Option<String>,
}

/// The zero-knowledge proof consisting of three elliptic curve points.
#[derive(SimpleObject, Clone)]
pub(crate) struct ZkLoginProof {
    /// G1 point 'a'.
    a: Option<CircomG1>,
    /// G2 point 'b'.
    b: Option<CircomG2>,
    /// G1 point 'c'.
    c: Option<CircomG1>,
}

/// A G1 elliptic curve point with 3 base10-encoded Bn254 field elements.
#[derive(SimpleObject, Clone)]
pub(crate) struct CircomG1 {
    e0: Option<String>,
    e1: Option<String>,
    e2: Option<String>,
}

/// A G2 elliptic curve point with 6 base10-encoded Bn254 field elements.
#[derive(SimpleObject, Clone)]
pub(crate) struct CircomG2 {
    e00: Option<String>,
    e01: Option<String>,
    e10: Option<String>,
    e11: Option<String>,
    e20: Option<String>,
    e21: Option<String>,
}

/// A Base64-encoded claim from the JWT used in zkLogin.
#[derive(SimpleObject, Clone)]
pub(crate) struct ZkLoginClaim {
    /// The Base64url-unpadded encoded claim value.
    value: Option<String>,
    /// The index mod 4 used for Base64 decoding alignment.
    index_mod_4: Option<u8>,
}

/// A JWK (JSON Web Key) identifier.
#[derive(SimpleObject, Clone)]
pub(crate) struct ZkLoginJwkId {
    /// The OIDC provider issuer string.
    iss: Option<String>,
    /// The key ID that identifies the JWK.
    kid: Option<String>,
}
