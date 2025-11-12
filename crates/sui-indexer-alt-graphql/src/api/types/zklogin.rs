// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, Enum, SimpleObject};
use im::hashmap::{Entry, HashMap};
use serde::Serialize;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_types::{
    SUI_AUTHENTICATOR_STATE_ADDRESS, TypeTag,
    authenticator_state::{ActiveJwk, AuthenticatorStateInner},
    crypto::ToFromBytes,
    dynamic_field::DynamicFieldType,
    signature::{GenericSignature, VerifyParams},
    signature_verification::VerifiedDigestCache,
    transaction::TransactionData,
};
use tracing::warn;

use crate::{
    api::{
        scalars::{base64::Base64, sui_address::SuiAddress, type_filter::TypeInput},
        types::dynamic_field::{DynamicField, DynamicFieldName},
    },
    config::ZkLoginConfig,
    error::{RpcError, bad_user_input, upcast},
    scope::Scope,
};

use super::epoch::Epoch;

/// An enum that specifies the intent scope to be used to parse the bytes for signature verification.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ZkLoginIntentScope {
    /// Indicates that the bytes are to be parsed as transaction data bytes.
    TransactionData,
    /// Indicates that the bytes are to be parsed as a personal message.
    PersonalMessage,
}

/// The result of the zkLogin signature verification.
#[derive(SimpleObject, Clone, Debug)]
pub(crate) struct ZkLoginVerifyResult {
    /// The boolean result of the verification. If true, errors should be empty.
    pub success: Option<bool>,
    /// The error field capture reasons why the signature could not be verified, assuming the inputs are valid and there are no internal errors.
    pub error: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Cannot parse signature")]
    BadSignature,

    #[error("Not a zkLogin signature")]
    NotZkLogin,

    #[error("Failed to deserialize TransactionData from bytes")]
    NotTransactionData,
}

/// Verified a zkLogin signature is from the given `author`.
///
/// `bytes` are either a serialized `TransactionData` or a personal message, depending on
/// `intent_scope`.
pub(crate) async fn verify_signature(
    ctx: &Context<'_>,
    scope: Scope,
    Base64(bytes): Base64,
    Base64(signature): Base64,
    intent_scope: ZkLoginIntentScope,
    author: SuiAddress,
) -> Result<ZkLoginVerifyResult, RpcError<Error>> {
    let config: &ZkLoginConfig = ctx.data()?;

    let epoch = Epoch::fetch(ctx, scope.clone(), None)
        .await
        .map_err(upcast)?
        .context("Failed to fetch current Epoch")?;

    let sig @ GenericSignature::ZkLoginAuthenticator(_) = GenericSignature::from_bytes(&signature)
        .map_err(|_| bad_user_input(Error::BadSignature))?
    else {
        return Err(bad_user_input(Error::NotZkLogin));
    };

    let jwk_object = DynamicField::by_name(
        ctx,
        scope,
        SUI_AUTHENTICATOR_STATE_ADDRESS.into(),
        DynamicFieldType::DynamicField,
        DynamicFieldName {
            type_: TypeInput(TypeTag::U64),
            bcs: Base64(bcs::to_bytes(&1u64).unwrap()),
        },
    )
    .await
    .map_err(upcast)?
    .context("JWK dynamic field not found")?;

    let authenticator_field = jwk_object
        .native(ctx)
        .await
        .map_err(upcast)?
        .as_ref()
        .context("Couldn't fetch JWK dynamic field contents")?;

    let authenticator_jwks: AuthenticatorStateInner =
        bcs::from_bytes(&authenticator_field.value_bytes)
            .context("Failed to deserialize JWK dynamic field contents")?;

    let mut jwks = HashMap::new();
    for ActiveJwk { jwk_id, jwk, .. } in authenticator_jwks.active_jwks {
        match jwks.entry(jwk_id.clone()) {
            Entry::Occupied(_) => {
                warn!("JWK with kid {jwk_id:?} already exists, skipping");
            }
            Entry::Vacant(entry) => {
                entry.insert(jwk.clone());
            }
        }
    }

    let params = VerifyParams::new(
        jwks,
        vec![],
        config.env,
        true,
        true,
        true,
        config.max_epoch_upper_bound_delta,
        true,
    );

    Ok(match intent_scope {
        ZkLoginIntentScope::TransactionData => verify(
            sig,
            &IntentMessage::new(
                Intent::sui_transaction(),
                bcs::from_bytes::<TransactionData>(&bytes)
                    .map_err(|_| bad_user_input(Error::NotTransactionData))?,
            ),
            author,
            epoch.epoch_id,
            &params,
        ),

        ZkLoginIntentScope::PersonalMessage => verify(
            sig,
            &IntentMessage::new(
                Intent::personal_message(),
                PersonalMessage { message: bytes },
            ),
            author,
            epoch.epoch_id,
            &params,
        ),
    })
}

fn verify<T: Serialize>(
    sig: GenericSignature,
    message: &IntentMessage<T>,
    author: SuiAddress,
    epoch_id: u64,
    params: &VerifyParams,
) -> ZkLoginVerifyResult {
    match sig.verify_authenticator(
        message,
        author.into(),
        epoch_id,
        params,
        Arc::new(VerifiedDigestCache::new_empty()),
    ) {
        Ok(()) => ZkLoginVerifyResult {
            success: Some(true),
            error: None,
        },

        Err(e) => ZkLoginVerifyResult {
            success: Some(false),
            error: Some(e.to_string()),
        },
    }
}
