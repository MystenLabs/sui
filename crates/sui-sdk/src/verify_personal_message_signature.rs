// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::error::Error;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_rpc::proto::sui::rpc::v2::{Bcs, UserSignature, VerifySignatureRequest};
use sui_rpc_api::Client;
use sui_types::{
    SUI_ADDRESS_ALIAS_STATE_OBJECT_ID, SUI_FRAMEWORK_ADDRESS,
    base_types::SuiAddress,
    derived_object,
    signature::{AuthenticatorTrait, GenericSignature, VerifyParams},
    signature_verification::VerifiedDigestCache,
};

/// Verify a signature against a personal message bytes and the sui address.
/// SuiClient is required to pass in if zkLogin signature is supplied.
pub async fn verify_personal_message_signature(
    signature: GenericSignature,
    message: &[u8],
    address: SuiAddress,
    client: Option<Client>,
) -> Result<(), Error> {
    // If client is provided, check if the address has aliases enabled - if so, reject verification
    if let Some(mut client_clone) = client.clone()
        && has_address_aliases(&mut client_clone, address).await?
    {
        return Err(Error::InvalidSignature);
    }

    let intent_msg = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: message.to_vec(),
        },
    );
    match signature {
        GenericSignature::ZkLoginAuthenticator(ref _sig) => {
            if let Some(mut client) = client {
                let message = Bcs::serialize(&message)?.with_name("PersonalMessage");
                let user_signature =
                    UserSignature::default().with_bcs(Bcs::from(signature.as_ref().to_owned()));

                let res = client
                    .inner_mut()
                    .signature_verification_client()
                    .verify_signature(
                        VerifySignatureRequest::default()
                            .with_address(address.to_string())
                            .with_message(message)
                            .with_signature(user_signature),
                    )
                    .await
                    .map_err(|_| Error::InvalidSignature)?
                    .into_inner();

                if res.is_valid() {
                    Ok(())
                } else {
                    Err(Error::InvalidSignature)
                }
            } else {
                Err(Error::InvalidSignature)
            }
        }
        _ => signature
            .verify_claims::<PersonalMessage>(
                &intent_msg,
                address,
                &VerifyParams::default(),
                Arc::new(VerifiedDigestCache::new_empty()),
            )
            .map_err(|_| Error::InvalidSignature),
    }
}

async fn has_address_aliases(client: &mut Client, address: SuiAddress) -> Result<bool, Error> {
    let alias_key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: Identifier::new("address_alias").unwrap(),
        name: Identifier::new("AliasKey").unwrap(),
        type_params: vec![],
    }));

    let key_bytes = bcs::to_bytes(&address).unwrap();
    let address_aliases_id = derived_object::derive_object_id(
        SuiAddress::from(SUI_ADDRESS_ALIAS_STATE_OBJECT_ID),
        &alias_key_type,
        &key_bytes,
    )
    .map_err(|_| Error::InvalidSignature)?;

    match client.get_object(address_aliases_id).await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
