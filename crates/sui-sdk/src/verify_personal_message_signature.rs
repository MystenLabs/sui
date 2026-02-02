// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::error::Error;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_rpc::proto::sui::rpc::v2::{Bcs, UserSignature, VerifySignatureRequest};
use sui_rpc_api::Client;
use sui_types::{
    base_types::SuiAddress,
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
