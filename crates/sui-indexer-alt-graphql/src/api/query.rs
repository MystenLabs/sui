// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object};
use futures::future::try_join_all;
use sui_types::digests::ChainIdentifier;

use crate::error::RpcError;

use super::{
    scalars::{digest::Digest, sui_address::SuiAddress, uint53::UInt53},
    types::{
        object::{self, Object, ObjectKey},
        service_config::ServiceConfig,
        transaction::Transaction,
        transaction_effects::TransactionEffects,
    },
};

pub struct Query;

#[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the network), hex-encoded.
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String, RpcError> {
        let chain_id: ChainIdentifier = *ctx.data()?;
        Ok(chain_id.to_string())
    }

    /// Fetch objects by their keys.
    ///
    /// Returns a list of objects that is guaranteed to be the same length as `keys`. If an object in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the object never existed, or because it was pruned.
    async fn multi_get_objects(
        &self,
        ctx: &Context<'_>,
        keys: Vec<ObjectKey>,
    ) -> Result<Vec<Option<Object>>, RpcError<object::Error>> {
        let objects = keys.into_iter().map(|k| Object::by_key(ctx, k));
        try_join_all(objects).await
    }

    /// Fetch transactions by their digests.
    ///
    /// Returns a list of transactions that is guaranteed to be the same length as `keys`. If a digest in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the transaction never existed, or because it was pruned.
    async fn multi_get_transactions(
        &self,
        ctx: &Context<'_>,
        keys: Vec<Digest>,
    ) -> Result<Vec<Option<Transaction>>, RpcError> {
        let transactions = keys.into_iter().map(|d| Transaction::fetch(ctx, d));
        try_join_all(transactions).await
    }

    /// Fetch transaction effects by their transactions' digests.
    ///
    /// Returns a list of transaction effects that is guaranteed to be the same length as `keys`. If a digest in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the transaction effects never existed, or because it was pruned.
    async fn multi_get_transaction_effects(
        &self,
        ctx: &Context<'_>,
        keys: Vec<Digest>,
    ) -> Result<Vec<Option<TransactionEffects>>, RpcError> {
        let effects = keys.into_iter().map(|d| TransactionEffects::fetch(ctx, d));
        try_join_all(effects).await
    }

    /// Fetch an object by its address.
    ///
    /// If `version` is specified, the object will be fetched at that exact version.
    ///
    /// If `rootVersion` is specified, the object will be fetched at the latest version at or before this version. This can be used to fetch a child or ancestor object bounded by its root object's version. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    ///
    /// It is an error to specify both `version` and `rootVersion`, or to specify neither.
    ///
    /// Returns `null` if an object cannot be found that meets this criteria.
    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        Object::by_key(
            ctx,
            ObjectKey {
                address,
                version,
                root_version,
            },
        )
        .await
    }

    /// Configuration for this RPC service.
    async fn service_config(&self) -> ServiceConfig {
        ServiceConfig
    }

    /// Fetch a transaction by its digest.
    ///
    /// Returns `null` if the transaction does not exist in the store, either because it never existed or because it was pruned.
    async fn transaction(
        &self,
        ctx: &Context<'_>,
        digest: Digest,
    ) -> Result<Option<Transaction>, RpcError> {
        Transaction::fetch(ctx, digest).await
    }

    /// Fetch transaction effects by its transaction's digest.
    ///
    /// Returns `null` if the transaction effects do not exist in the store, either because that transaction was not executed, or it was pruned.
    async fn transaction_effects(
        &self,
        ctx: &Context<'_>,
        digest: Digest,
    ) -> Result<Option<TransactionEffects>, RpcError> {
        TransactionEffects::fetch(ctx, digest).await
    }
}
