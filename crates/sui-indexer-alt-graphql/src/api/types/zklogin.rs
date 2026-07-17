// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Enum;
use async_graphql::SimpleObject;
use serde::Serialize;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use shared_crypto::intent::PersonalMessage;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::signature::VerifyParams;
use sui_types::signature_verification::VerifiedDigestCache;
use sui_types::transaction::TransactionData;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::types::epoch::Epoch;
use crate::api::types::signature_verify::chain_zklogin_circuit_mode;
use crate::api::types::signature_verify::fetch_jwks;
use crate::config::ZkLoginConfig;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::scope::Scope;

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
    /// Whether the signature was verified successfully.
    pub success: Option<bool>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Cannot parse signature")]
    BadSignature,

    #[error("Not a zkLogin signature")]
    NotZkLogin,

    #[error("Failed to deserialize TransactionData from bytes")]
    NotTransactionData,

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
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

    let jwks = fetch_jwks(ctx, scope).await.map_err(upcast)?;

    let zklogin_circuit_mode = chain_zklogin_circuit_mode(ctx, &epoch)
        .await
        .map_err(upcast)?;

    let params = VerifyParams::new(
        jwks,
        vec![],
        config.env,
        zklogin_circuit_mode,
        true,
        true,
        true,
        config.max_epoch_upper_bound_delta,
        true,
        true,
    );

    match intent_scope {
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
    }
    .map_err(|e| bad_user_input(Error::VerificationFailed(e.to_string())))?;

    Ok(ZkLoginVerifyResult {
        success: Some(true),
    })
}

fn verify<T: Serialize>(
    sig: GenericSignature,
    message: &IntentMessage<T>,
    author: SuiAddress,
    epoch_id: u64,
    params: &VerifyParams,
) -> Result<(), sui_types::error::SuiError> {
    sig.verify_authenticator(
        message,
        author.into(),
        epoch_id,
        params,
        Arc::new(VerifiedDigestCache::new_empty()),
    )
}
