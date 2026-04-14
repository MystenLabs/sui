// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Enum;
use async_graphql::SimpleObject;
use im::hashmap::Entry;
use im::hashmap::HashMap;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use shared_crypto::intent::PersonalMessage;
use sui_types::SUI_AUTHENTICATOR_STATE_ADDRESS;
use sui_types::TypeTag;
use sui_types::authenticator_state::ActiveJwk;
use sui_types::authenticator_state::AuthenticatorStateInner;
use sui_types::crypto::ToFromBytes;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::signature::GenericSignature;
use sui_types::signature::VerifyParams;
use sui_types::signature_verification::VerifiedDigestCache;
use sui_types::transaction::TransactionData;
use tracing::warn;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeInput;
use crate::api::types::dynamic_field::DynamicField;
use crate::api::types::epoch::Epoch;
use crate::config::ZkLoginConfig;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::scope::Scope;

/// An enum that specifies the intent scope for signature verification.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum IntentScope {
    /// Indicates that the bytes are to be parsed as transaction data bytes.
    TransactionData,
    /// Indicates that the bytes are to be parsed as a personal message.
    PersonalMessage,
}

/// The result of signature verification.
#[derive(SimpleObject, Clone, Debug)]
pub(crate) struct SignatureVerifyResult {
    /// Whether the signature was verified successfully.
    pub success: Option<bool>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Cannot parse signature")]
    BadSignature,

    #[error("Failed to deserialize TransactionData from bytes")]
    NotTransactionData,

    #[error("Verification failed: {0}")]
    VerificationFailed(String),
}

/// Fetch active JWKs from the on-chain authenticator state. This reads from the indexed store
/// to maintain checkpoint consistency with other GraphQL queries.
pub(crate) async fn fetch_jwks(
    ctx: &Context<'_>,
    scope: Scope,
) -> Result<
    HashMap<fastcrypto_zkp::bn254::zk_login::JwkId, fastcrypto_zkp::bn254::zk_login::JWK>,
    RpcError,
> {
    let jwk_object = DynamicField::by_serialized_name(
        ctx,
        scope,
        SUI_AUTHENTICATOR_STATE_ADDRESS.into(),
        DynamicFieldType::DynamicField,
        TypeInput(TypeTag::U64),
        Base64(bcs::to_bytes(&1u64).unwrap()),
    )
    .await?
    .context("JWK dynamic field not found")?;

    let authenticator_field = jwk_object
        .native(ctx)
        .await?
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

    Ok(jwks)
}

/// Verify any signature type locally. Supports Ed25519, Secp256k1, Secp256r1, MultiSig, ZkLogin,
/// and Passkey.
pub(crate) async fn verify_signature(
    ctx: &Context<'_>,
    scope: Scope,
    Base64(message): Base64,
    Base64(signature): Base64,
    intent_scope: IntentScope,
    author: SuiAddress,
) -> Result<SignatureVerifyResult, RpcError<Error>> {
    let config: &ZkLoginConfig = ctx.data()?;

    let sig = GenericSignature::from_bytes(&signature)
        .map_err(|_| bad_user_input(Error::BadSignature))?;

    let epoch = Epoch::fetch(ctx, scope.clone(), None)
        .await
        .map_err(upcast)?
        .context("Failed to fetch current Epoch")?;

    let jwks = fetch_jwks(ctx, scope).await.map_err(upcast)?;

    let params = VerifyParams::new(
        jwks,
        vec![],
        config.env,
        true,
        true,
        true,
        config.max_epoch_upper_bound_delta,
        true,
        true,
    );

    match intent_scope {
        IntentScope::TransactionData => sig.verify_authenticator(
            &IntentMessage::new(
                Intent::sui_transaction(),
                bcs::from_bytes::<TransactionData>(&message)
                    .map_err(|_| bad_user_input(Error::NotTransactionData))?,
            ),
            author.into(),
            epoch.epoch_id,
            &params,
            Arc::new(VerifiedDigestCache::new_empty()),
        ),
        IntentScope::PersonalMessage => sig.verify_authenticator(
            &IntentMessage::new(Intent::personal_message(), PersonalMessage { message }),
            author.into(),
            epoch.epoch_id,
            &params,
            Arc::new(VerifiedDigestCache::new_empty()),
        ),
    }
    .map_err(|e| bad_user_input(Error::VerificationFailed(e.to_string())))?;

    Ok(SignatureVerifyResult {
        success: Some(true),
    })
}
