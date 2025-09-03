// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_crypto::Verifier;
use sui_sdk_types::Jwk;
use sui_sdk_types::JwkId;
use tap::Pipe;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::signature_verification_service_server::SignatureVerificationService;
use sui_rpc::proto::sui::rpc::v2beta2::VerifySignatureRequest;
use sui_rpc::proto::sui::rpc::v2beta2::VerifySignatureResponse;

#[tonic::async_trait]
impl SignatureVerificationService for RpcService {
    async fn verify_signature(
        &self,
        request: tonic::Request<VerifySignatureRequest>,
    ) -> Result<tonic::Response<VerifySignatureResponse>, tonic::Status> {
        verify_signature(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

#[tracing::instrument(skip(service))]
fn verify_signature(
    service: &RpcService,
    request: VerifySignatureRequest,
) -> Result<VerifySignatureResponse> {
    let signature = request
        .signature
        .as_ref()
        .ok_or_else(|| FieldViolation::new("signature").with_reason(ErrorReason::FieldMissing))?
        .pipe(sui_sdk_types::UserSignature::try_from)
        .map_err(|e| {
            FieldViolation::new("signature")
                .with_description(format!("invalid signature: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let signing_digest = {
        let bcs = request
            .message
            .ok_or_else(|| FieldViolation::new("message").with_reason(ErrorReason::FieldMissing))?;

        match bcs.name() {
            "TransactionData" => bcs
                .deserialize::<sui_sdk_types::Transaction>()?
                .signing_digest(),
            "PersonalMessage" => bcs
                .deserialize::<&[u8]>()
                .map(|slice| sui_sdk_types::PersonalMessage(slice.into()))?
                .signing_digest(),
            _ => {
                if let Ok(personal_message) = bcs
                    .deserialize::<&[u8]>()
                    .map(|slice| sui_sdk_types::PersonalMessage(slice.into()))
                {
                    personal_message.signing_digest()
                } else if let Ok(transaction) = bcs.deserialize::<sui_sdk_types::Transaction>() {
                    transaction.signing_digest()
                } else {
                    return Err(FieldViolation::new("message")
                        .with_description("invalid message")
                        .with_reason(ErrorReason::FieldInvalid)
                        .into());
                }
            }
        }
    };

    if let Some(address) = request
        .address
        .map(|address| address.parse::<sui_sdk_types::Address>())
        .transpose()
        .map_err(|e| {
            FieldViolation::new("address")
                .with_description(format!("invalid address: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?
    {
        //TODO add function in sui_sdk_types crate to do this
        let derived_addresses = match &signature {
            sui_sdk_types::UserSignature::Simple(simple_signature) => match simple_signature {
                sui_sdk_types::SimpleSignature::Ed25519 { public_key, .. } => {
                    [Some(public_key.derive_address()), None]
                }
                sui_sdk_types::SimpleSignature::Secp256k1 { public_key, .. } => {
                    [Some(public_key.derive_address()), None]
                }
                sui_sdk_types::SimpleSignature::Secp256r1 { public_key, .. } => {
                    [Some(public_key.derive_address()), None]
                }
                _ => {
                    return Err(RpcError::new(
                        tonic::Code::Internal,
                        "unknown signature scheme",
                    ))
                }
            },
            sui_sdk_types::UserSignature::Multisig(multisig) => {
                [Some(multisig.committee().derive_address()), None]
            }
            sui_sdk_types::UserSignature::ZkLogin(z) => {
                let id = z.inputs.public_identifier();
                [
                    Some(id.derive_address_padded()),
                    Some(id.derive_address_unpadded()),
                ]
            }
            sui_sdk_types::UserSignature::Passkey(p) => {
                [Some(p.public_key().derive_address()), None]
            }
            _ => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    "unknown signature scheme",
                ))
            }
        };

        let first_derived_address = derived_addresses[0].unwrap();

        // If none of the possible derived addresses match we need to return that this is invalid
        if !derived_addresses
            .into_iter()
            .flatten()
            .any(|derived_address| derived_address == address)
        {
            let mut message = VerifySignatureResponse::default();
            message.is_valid = Some(false);
            message.reason = Some(format!(
                "provided address `{}` does not match derived address `{}`",
                address, first_derived_address
            ));
            return Ok(message);
        }
    }

    // If jwks from the request is empty we load the current set of active jwks that are onchain
    let jwks = {
        let mut jwks = request
            .jwks
            .iter()
            .enumerate()
            .map(|(i, jwk)| {
                let jwk = sui_sdk_types::ActiveJwk::try_from(jwk).map_err(|e| {
                    FieldViolation::new_at("jwks", i)
                        .with_description(e.to_string())
                        .with_reason(ErrorReason::FieldInvalid)
                })?;
                Ok((jwk.jwk_id, jwk.jwk))
            })
            .collect::<Result<HashMap<JwkId, Jwk>>>()?;

        if jwks.is_empty() {
            if let Some(authenticator_state) = service.reader.get_authenticator_state()? {
                jwks.extend(
                    authenticator_state
                        .active_jwks
                        .into_iter()
                        .map(sui_sdk_types::ActiveJwk::from)
                        .map(|active_jwk| (active_jwk.jwk_id, active_jwk.jwk)),
                );
            }
        }

        jwks
    };

    let mut zklogin_verifier = match service.chain_id().chain() {
        sui_protocol_config::Chain::Mainnet => sui_crypto::zklogin::ZkloginVerifier::new_mainnet(),
        sui_protocol_config::Chain::Testnet | sui_protocol_config::Chain::Unknown => {
            sui_crypto::zklogin::ZkloginVerifier::new_dev()
        }
    };
    *zklogin_verifier.jwks_mut() = jwks;
    let mut verifier = sui_crypto::UserSignatureVerifier::new();
    verifier.with_zklogin_verifier(zklogin_verifier);

    let mut message = VerifySignatureResponse::default();
    match verifier.verify(&signing_digest, &signature) {
        Ok(()) => message.is_valid = Some(true),
        Err(error) => {
            message.is_valid = Some(false);
            message.reason = Some(error.to_string());
        }
    }

    Ok(message)
}
