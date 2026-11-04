// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Enum;
use async_graphql::SimpleObject;
use futures::future::OptionFuture;
use futures::join;
use imbl::hashmap::Entry;
use imbl::hashmap::HashMap;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use shared_crypto::intent::PersonalMessage;
use sui_types::SUI_AUTHENTICATOR_STATE_ADDRESS;
use sui_types::TypeTag;
use sui_types::address_alias::AddressAliases;
use sui_types::address_alias::address_aliases_object_id;
use sui_types::authenticator_state::ActiveJwk;
use sui_types::authenticator_state::AuthenticatorStateInner;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
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
use crate::api::types::object::Object;
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

    #[error("Verification failed: signature from {signer} is not an alias of {author}")]
    NotAnAlias {
        signer: NativeSuiAddress,
        author: NativeSuiAddress,
    },
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

/// Verification params to use for checks in RPC. These parameters are more permissive than how the
/// chain may be configured at any one point in time, because the RPC at time T needs to accept
/// requests to verify signatures from all times less than or equal to T.
pub(crate) fn verification_params(
    config: &ZkLoginConfig,
    jwks: HashMap<fastcrypto_zkp::bn254::zk_login::JwkId, fastcrypto_zkp::bn254::zk_login::JWK>,
) -> VerifyParams {
    VerifyParams::new(
        jwks,
        vec![],
        config.env,
        1, // Both v1 and v2 zkLogin circuits.
        true,
        true,
        true,
        config.max_epoch_upper_bound_delta,
        true,
        true,
    )
    .with_mldsa65_in_multisig(true)
}

/// Verify any supported signature type locally.
pub(crate) async fn verify_signature(
    ctx: &Context<'_>,
    scope: Scope,
    Base64(message): Base64,
    Base64(signature): Base64,
    intent_scope: IntentScope,
    author: SuiAddress,
    check_address_aliases: bool,
) -> Result<SignatureVerifyResult, RpcError<Error>> {
    let config: &ZkLoginConfig = ctx.data()?;

    let sig = GenericSignature::from_bytes(&signature)
        .map_err(|_| bad_user_input(Error::BadSignature))?;

    let epoch = Epoch::fetch(ctx, scope.clone(), None)
        .await
        .map_err(upcast)?
        .context("Failed to fetch current Epoch")?;

    let aliases: OptionFuture<_> = check_address_aliases
        .then(|| address_aliases(ctx, scope.clone(), author))
        .into();
    let (jwks, aliases) = join!(fetch_jwks(ctx, scope), aliases);
    let jwks = jwks.map_err(upcast)?;

    let signer = match aliases.transpose().map_err(upcast)? {
        Some(aliases) => {
            let derived =
                derived_addresses(&sig).map_err(|_| bad_user_input(Error::BadSignature))?;
            *derived
                .iter()
                .find(|address| aliases.contains(address))
                .ok_or_else(|| {
                    bad_user_input(Error::NotAnAlias {
                        signer: derived[0],
                        author: author.into(),
                    })
                })?
        }
        None => author.into(),
    };

    let params = verification_params(config, jwks);

    match intent_scope {
        IntentScope::TransactionData => sig.verify_authenticator(
            &IntentMessage::new(
                Intent::sui_transaction(),
                bcs::from_bytes::<TransactionData>(&message)
                    .map_err(|_| bad_user_input(Error::NotTransactionData))?,
            ),
            signer,
            epoch.epoch_id,
            &params,
            Arc::new(VerifiedDigestCache::new_empty()),
        ),
        IntentScope::PersonalMessage => sig.verify_authenticator(
            &IntentMessage::new(Intent::personal_message(), PersonalMessage { message }),
            signer,
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

/// A configured alias set replaces `address`; without one, `address` is its own sole alias.
async fn address_aliases(
    ctx: &Context<'_>,
    scope: Scope,
    address: SuiAddress,
) -> Result<Vec<NativeSuiAddress>, RpcError> {
    let aliases_id = address_aliases_object_id(address.into())
        .context("Failed to derive address aliases object ID")?;

    let Some(object) = Object::latest(ctx, scope.without_root_bound(), aliases_id.into()).await?
    else {
        return Ok(vec![address.into()]);
    };

    let contents = object
        .contents(ctx)
        .await?
        .context("Failed to fetch address aliases object contents")?
        .data
        .try_as_move()
        .context("Address aliases object is not a Move object")?;

    let aliases: AddressAliases = bcs::from_bytes(contents.contents())
        .context("Failed to deserialize address aliases object")?;

    Ok(aliases.aliases.contents)
}

/// The addresses `signature` can sign for. zkLogin signatures cover both the unpadded and padded
/// address derivations.
fn derived_addresses(signature: &GenericSignature) -> anyhow::Result<Vec<NativeSuiAddress>> {
    let mut addresses = vec![NativeSuiAddress::try_from(signature)?];
    if let GenericSignature::ZkLoginAuthenticator(zklogin) = signature {
        addresses.push(NativeSuiAddress::try_from_padded(&zklogin.inputs)?);
    }
    Ok(addresses)
}
