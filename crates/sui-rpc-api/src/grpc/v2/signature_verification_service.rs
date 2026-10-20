// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_crypto::Verifier;
use sui_crypto::zklogin::ZkLoginCircuitMode;
use sui_protocol_config::ProtocolConfig;
use sui_sdk_types::Jwk;
use sui_sdk_types::JwkId;
use tap::Pipe;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::VerifySignatureRequest;
use sui_rpc::proto::sui::rpc::v2::VerifySignatureResponse;
use sui_rpc::proto::sui::rpc::v2::signature_verification_service_server::SignatureVerificationService;

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

    let address = request
        .address
        .map(|address| address.parse::<sui_sdk_types::Address>())
        .transpose()
        .map_err(|e| {
            FieldViolation::new("address")
                .with_description(format!("invalid address: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let check_address_aliases = request.check_address_aliases.unwrap_or(false);
    if check_address_aliases && address.is_none() {
        return Err(FieldViolation::new("address")
            .with_description("address is required when check_address_aliases is true")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }

    let mut message = VerifySignatureResponse::default();
    // Lets clients detect older servers that ignore `check_address_aliases`.
    if check_address_aliases {
        message.address_aliases_checked = Some(true);
    }

    if let Some(address) = address {
        let derived_addresses = derived_addresses(&signature)?;
        let first_derived_address = derived_addresses[0].unwrap();

        let authorized_addresses = if check_address_aliases {
            current_address_aliases(service, address)?
        } else {
            vec![address]
        };

        if !derived_addresses
            .into_iter()
            .flatten()
            .any(|derived_address| authorized_addresses.contains(&derived_address))
        {
            message.is_valid = Some(false);
            message.reason = Some(if check_address_aliases {
                format!("derived address `{first_derived_address}` is not an alias of `{address}`")
            } else {
                format!(
                    "provided address `{address}` does not match derived address `{first_derived_address}`"
                )
            });
            return Ok(message);
        }
    }

    // Building the zklogin verifier when the signature can actually contain a zklogin signature.
    let mut verifier = sui_crypto::UserSignatureVerifier::new();
    if signature_may_contain_zklogin(&signature) {
        verifier.with_zklogin_verifier(build_zklogin_verifier(service, &request.jwks)?);
    }

    match verifier.verify(&signing_digest, &signature) {
        Ok(()) => message.is_valid = Some(true),
        Err(error) => {
            message.is_valid = Some(false);
            message.reason = Some(error.to_string());
        }
    }

    Ok(message)
}

fn derived_addresses(
    signature: &sui_sdk_types::UserSignature,
) -> Result<[Option<sui_sdk_types::Address>; 2]> {
    //TODO add function in sui_sdk_types crate to do this
    let derived_addresses = match signature {
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
            sui_sdk_types::SimpleSignature::MlDsa65 { public_key, .. } => {
                [Some(public_key.derive_address()), None]
            }
            _ => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    "unknown signature scheme",
                ));
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
        sui_sdk_types::UserSignature::Passkey(p) => [Some(p.public_key().derive_address()), None],
        _ => {
            return Err(RpcError::new(
                tonic::Code::Internal,
                "unknown signature scheme",
            ));
        }
    };
    Ok(derived_addresses)
}

/// A configured alias set replaces `address`; without one, `address` is its own sole alias.
fn current_address_aliases(
    service: &RpcService,
    address: sui_sdk_types::Address,
) -> Result<Vec<sui_sdk_types::Address>> {
    if !current_protocol_config(service)?.address_aliases() {
        return Ok(vec![address]);
    }

    let Some(aliases) = service.reader.get_address_aliases(address.into())? else {
        return Ok(vec![address]);
    };

    Ok(aliases
        .aliases
        .contents
        .into_iter()
        .map(sui_sdk_types::Address::from)
        .collect())
}

fn current_protocol_config(service: &RpcService) -> Result<ProtocolConfig> {
    let chain = service.chain_id().chain();
    let system_state = service.reader.get_system_state_summary()?;
    ProtocolConfig::get_for_version_if_supported(system_state.protocol_version.into(), chain)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "protocol version {} is not supported",
                    system_state.protocol_version
                ),
            )
        })
}

fn signature_may_contain_zklogin(signature: &sui_sdk_types::UserSignature) -> bool {
    match signature {
        sui_sdk_types::UserSignature::ZkLogin(_) => true,
        sui_sdk_types::UserSignature::Multisig(multisig) => {
            multisig.committee().members().iter().any(|member| {
                matches!(
                    member.public_key(),
                    sui_sdk_types::MultisigMemberPublicKey::ZkLogin(_)
                )
            })
        }
        _ => false,
    }
}

fn build_zklogin_verifier(
    service: &RpcService,
    request_jwks: &[sui_rpc::proto::sui::rpc::v2::ActiveJwk],
) -> Result<sui_crypto::zklogin::ZkloginVerifier> {
    // If jwks from the request is empty we load the current set of active jwks that are onchain
    let jwks = {
        let mut jwks = request_jwks
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

        if jwks.is_empty()
            && let Some(authenticator_state) = service.reader.get_authenticator_state()?
        {
            jwks.extend(
                authenticator_state
                    .active_jwks
                    .into_iter()
                    .map(sui_sdk_types::ActiveJwk::from)
                    .map(|active_jwk| (active_jwk.jwk_id, active_jwk.jwk)),
            );
        }

        jwks
    };

    let chain = service.chain_id().chain();
    let mut zklogin_verifier = match chain {
        sui_protocol_config::Chain::Mainnet | sui_protocol_config::Chain::Testnet => {
            sui_crypto::zklogin::ZkloginVerifier::new_mainnet()
        }
        sui_protocol_config::Chain::Unknown => sui_crypto::zklogin::ZkloginVerifier::new_dev(),
    };

    zklogin_verifier.set_circuit_mode(
        match current_protocol_config(service)?.zklogin_circuit_mode() {
            0 => ZkLoginCircuitMode::V1Only,
            1 => ZkLoginCircuitMode::Both,
            2 => ZkLoginCircuitMode::V2Only,
            mode => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    format!("invalid zklogin circuit mode in protocol config: {mode}"),
                ));
            }
        },
    );
    *zklogin_verifier.jwks_mut() = jwks;
    Ok(zklogin_verifier)
}
