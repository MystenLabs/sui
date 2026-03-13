// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_crypto::Verifier;
use sui_sdk_types::Jwk;
use sui_sdk_types::JwkId;
use sui_types::address_alias;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
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
            sui_sdk_types::UserSignature::Passkey(p) => {
                [Some(p.public_key().derive_address()), None]
            }
            _ => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    "unknown signature scheme",
                ));
            }
        };

        let first_derived_address = derived_addresses[0].unwrap();

        // Check if any of the derived addresses match the provided address directly
        let direct_match = derived_addresses
            .into_iter()
            .flatten()
            .any(|derived_address| derived_address == address);

        if !direct_match {
            // Check if the signer is in the address's alias set
            let native_address: NativeSuiAddress = address.into();
            let aliases = address_alias::get_address_aliases_from_store(
                service.reader.inner(),
                native_address,
            )
            .map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("Failed to query aliases: {e}"),
                )
            })?;

            let signer_is_alias = if let Some((alias_set, _version)) = aliases {
                // Check if first_derived_address is in the alias set
                let native_signer: NativeSuiAddress = first_derived_address.into();
                alias_set.aliases.contents.contains(&native_signer)
            } else {
                // No alias set exists, only the address itself is valid
                false
            };

            if !signer_is_alias {
                let mut message = VerifySignatureResponse::default();
                message.is_valid = Some(false);
                message.reason = Some(format!(
                    "provided address `{}` does not match derived address `{}` and signer is not in alias set",
                    address, first_derived_address
                ));
                return Ok(message);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sui_protocol_config::Chain;
    use sui_types::crypto::{get_key_pair, AccountKeyPair, Signature};
    use sui_types::object::{MoveObject, Object, Owner};
    use sui_types::storage::{ObjectStore, RpcStateReader};
    use sui_types::{
        address_alias::AddressAliases, base_types::SuiAddress, collection_types::VecSet,
    };
    use sui_types::{SUI_ADDRESS_ALIAS_STATE_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};

    // Mock object store for testing
    struct MockObjectStore {
        alias_object: Option<Object>,
        alias_address: SuiAddress,
    }

    impl ObjectStore for MockObjectStore {
        fn get_object(&self, object_id: &sui_types::base_types::ObjectID) -> Option<Object> {
            use move_core_types::identifier::Identifier;
            use move_core_types::language_storage::{StructTag, TypeTag};
            use sui_types::derived_object;

            let alias_key_type = TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: Identifier::new("address_alias").unwrap(),
                name: Identifier::new("AliasKey").unwrap(),
                type_params: vec![],
            }));

            let key_bytes = bcs::to_bytes(&self.alias_address).unwrap();
            let expected_id = derived_object::derive_object_id(
                SuiAddress::from(SUI_ADDRESS_ALIAS_STATE_OBJECT_ID),
                &alias_key_type,
                &key_bytes,
            )
            .unwrap();

            if object_id == &expected_id {
                self.alias_object.clone()
            } else {
                None
            }
        }

        fn get_object_by_key(
            &self,
            _object_id: &sui_types::base_types::ObjectID,
            _version: sui_types::base_types::VersionNumber,
        ) -> Option<Object> {
            None
        }
    }

    // Mock reader for testing
    struct MockReader {
        object_store: std::sync::Arc<MockObjectStore>,
    }

    impl RpcStateReader for MockReader {}

    #[test]
    fn test_verify_signature_with_alias() {
        // Create two key pairs - account1 (the address) and account2 (the alias)
        let (account1_address, _account1_keypair): (_, AccountKeyPair) = get_key_pair();
        let (account2_address, account2_keypair): (_, AccountKeyPair) = get_key_pair();

        // Create a personal message and have account2 sign it
        let message = b"Test message for alias verification";
        let intent_msg = IntentMessage::new(
            Intent::personal_message(),
            PersonalMessage {
                message: message.to_vec(),
            },
        );

        let signature = Signature::new_secure(&intent_msg, &account2_keypair);

        // Create an AddressAliases object with account2 as an alias for account1
        let alias_set = AddressAliases {
            id: sui_types::id::UID::new(sui_types::id::ID::new([0; 32])),
            aliases: VecSet {
                contents: vec![account1_address, account2_address],
            },
        };

        // Create the alias object
        let alias_object_contents = bcs::to_bytes(&alias_set).unwrap();
        let move_object = MoveObject::new_from_execution(
            move_core_types::language_storage::TypeTag::Struct(Box::new(
                move_core_types::language_storage::StructTag {
                    address: SUI_FRAMEWORK_ADDRESS.into(),
                    module: move_core_types::identifier::Identifier::new("address_alias").unwrap(),
                    name: move_core_types::identifier::Identifier::new("AddressAliases").unwrap(),
                    type_params: vec![],
                },
            )),
            alias_object_contents,
        )
        .unwrap();

        let alias_object = Object::new_move(
            move_object,
            Owner::ConsensusAddressOwner {
                address: account1_address,
            },
            sui_types::base_types::TransactionDigest::random(),
        );

        // Create mock service
        let object_store = std::sync::Arc::new(MockObjectStore {
            alias_object: Some(alias_object),
            alias_address: account1_address,
        });

        let reader = std::sync::Arc::new(MockReader { object_store });

        let service = RpcService {
            reader,
            chain: Chain::Unknown,
        };

        // Create the verification request
        let message_bcs = sui_rpc::proto::sui::rpc::v2::Bcs::serialize(&message.as_ref())
            .unwrap()
            .with_name("PersonalMessage");

        let user_signature = sui_rpc::proto::sui::rpc::v2::UserSignature::default()
            .with_bcs(sui_rpc::proto::sui::rpc::v2::Bcs::from(signature.as_ref().to_vec()));

        let request = VerifySignatureRequest::default()
            .with_address(account1_address.to_string())
            .with_message(message_bcs)
            .with_signature(user_signature);

        // Call verify_signature directly
        let result = verify_signature(&service, request).unwrap();

        // Should succeed because account2 is in account1's alias set
        assert!(
            result.is_valid.unwrap_or(false),
            "Verification should succeed when signer is in alias set. Reason: {:?}",
            result.reason
        );
    }
}
