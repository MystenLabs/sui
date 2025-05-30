// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use sui_crypto::Verifier;
use sui_sdk_types::Jwk;
use sui_sdk_types::JwkId;
use tap::Pipe;

use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::ErrorReason;
use crate::Result;
use crate::{
    proto::rpc::v2alpha::{VerifySignatureRequest, VerifySignatureResponse},
    RpcService,
};

#[tracing::instrument(skip(service))]
pub fn verify_signature(
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
                if let Ok(transaction) = bcs.deserialize::<sui_sdk_types::Transaction>() {
                    transaction.signing_digest()
                } else if let Ok(personal_message) = bcs
                    .deserialize::<&[u8]>()
                    .map(|slice| sui_sdk_types::PersonalMessage(slice.into()))
                {
                    personal_message.signing_digest()
                } else {
                    return Err(FieldViolation::new("message")
                        .with_description("invalid message")
                        .with_reason(ErrorReason::FieldInvalid)
                        .into());
                }
            }
        }
    };

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

    match verifier.verify(&signing_digest, &signature) {
        Ok(()) => VerifySignatureResponse {
            is_valid: Some(true),
            reason: None,
        },
        Err(error) => VerifySignatureResponse {
            is_valid: Some(false),
            reason: Some(error.to_string()),
        },
    }
    .pipe(Ok)
}
