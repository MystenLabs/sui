// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object};
use futures::future::try_join_all;
use sui_types::digests::ChainIdentifier;

use crate::{error::RpcError, scope::Scope};

use super::{
    scalars::{digest::Digest, sui_address::SuiAddress, uint53::UInt53},
    types::{
        checkpoint::Checkpoint,
        epoch::Epoch,
        move_package::{self, MovePackage, PackageKey},
        object::{self, Object, ObjectKey},
        protocol_configs::ProtocolConfigs,
        service_config::ServiceConfig,
        transaction::Transaction,
        transaction_effects::TransactionEffects,
    },
};

#[derive(Default)]
pub struct Query {
    /// Queries will use this scope if it is populated, instead of creating a fresh scope from
    /// information in the request-wide [Context].
    pub(crate) scope: Option<Scope>,
}

#[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the network), hex-encoded.
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String, RpcError> {
        let chain_id: ChainIdentifier = *ctx.data()?;
        Ok(chain_id.to_string())
    }

    /// Fetch a checkpoint by its sequence number, or the latest checkpoint if no sequence number is provided.
    ///
    /// Returns `null` if the checkpoint does not exist in the store, either because it never existed or because it was pruned.
    async fn checkpoint(
        &self,
        ctx: &Context<'_>,
        sequence_number: Option<UInt53>,
    ) -> Result<Option<Checkpoint>, RpcError> {
        let scope = self.scope(ctx)?;
        let sequence_number = sequence_number
            .map(|s| s.into())
            .unwrap_or(scope.checkpoint_viewed_at());

        Ok(Checkpoint::with_sequence_number(scope, sequence_number))
    }

    /// Fetch an epoch by its ID, or fetch the latest epoch if no ID is provided.
    ///
    /// Returns `null` if the epoch does not exist yet, or was pruned.
    async fn epoch(
        &self,
        ctx: &Context<'_>,
        epoch_id: Option<UInt53>,
    ) -> Result<Option<Epoch>, RpcError> {
        let scope = self.scope(ctx)?;
        Epoch::fetch(ctx, scope, epoch_id).await
    }

    /// Fetch checkpoints by their sequence numbers.
    ///
    /// Returns a list of checkpoints that is guaranteed to be the same length as `keys`. If a checkpoint in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the checkpoint does not exist yet, or because it was pruned.
    async fn multi_get_checkpoints(
        &self,
        ctx: &Context<'_>,
        keys: Vec<UInt53>,
    ) -> Result<Vec<Option<Checkpoint>>, RpcError> {
        let scope = self.scope(ctx)?;
        Ok(keys
            .into_iter()
            .map(|k| Checkpoint::with_sequence_number(scope.clone(), k.into()))
            .collect())
    }

    /// Fetch epochs by their IDs.
    ///
    /// Returns a list of epochs that is guaranteed to be the same length as `keys`. If an epoch in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the epoch does not exist yet, or because it was pruned.
    async fn multi_get_epochs(
        &self,
        ctx: &Context<'_>,
        keys: Vec<UInt53>,
    ) -> Result<Vec<Option<Epoch>>, RpcError> {
        let scope = self.scope(ctx)?;
        let epochs = keys
            .into_iter()
            .map(|k| Epoch::fetch(ctx, scope.clone(), Some(k)));

        try_join_all(epochs).await
    }

    /// Fetch objects by their keys.
    ///
    /// Returns a list of objects that is guaranteed to be the same length as `keys`. If an object in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the object never existed, or because it was pruned.
    async fn multi_get_objects(
        &self,
        ctx: &Context<'_>,
        keys: Vec<ObjectKey>,
    ) -> Result<Vec<Option<Object>>, RpcError<object::Error>> {
        let scope = self.scope(ctx)?;
        let objects = keys
            .into_iter()
            .map(|k| Object::by_key(ctx, scope.clone(), k));

        try_join_all(objects).await
    }

    /// Fetch packages by their keys.
    ///
    /// Returns a list of packages that is guaranteed to be the same length as `keys`. If a package in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because that address never pointed to a package, or because the package was pruned.
    async fn multi_get_packages(
        &self,
        ctx: &Context<'_>,
        keys: Vec<PackageKey>,
    ) -> Result<Vec<Option<MovePackage>>, RpcError<move_package::Error>> {
        let scope = self.scope(ctx)?;
        let packages = keys
            .into_iter()
            .map(|k| MovePackage::by_key(ctx, scope.clone(), k));

        try_join_all(packages).await
    }

    /// Fetch transactions by their digests.
    ///
    /// Returns a list of transactions that is guaranteed to be the same length as `keys`. If a digest in `keys` could not be found in the store, its corresponding entry in the result will be `null`. This could be because the transaction never existed, or because it was pruned.
    async fn multi_get_transactions(
        &self,
        ctx: &Context<'_>,
        keys: Vec<Digest>,
    ) -> Result<Vec<Option<Transaction>>, RpcError> {
        let scope = self.scope(ctx)?;
        let transactions = keys
            .into_iter()
            .map(|d| Transaction::fetch(ctx, scope.clone(), d));

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
        let scope = self.scope(ctx)?;
        let effects = keys
            .into_iter()
            .map(|d| TransactionEffects::fetch(ctx, scope.clone(), d));

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
    /// If `atCheckpoint` is specified, the object will be fetched at the latest version as of this checkpoint.
    ///
    /// If none of the above are specified, the object is fetched at the latest checkpoint.
    ///
    /// It is an error to specify more than one of `version`, `rootVersion`, or `atCheckpoint`.
    ///
    /// Returns `null` if an object cannot be found that meets this criteria.
    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        at_checkpoint: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        Object::by_key(
            ctx,
            self.scope(ctx)?,
            ObjectKey {
                address,
                version,
                root_version,
                at_checkpoint,
            },
        )
        .await
    }

    /// Fetch a package by its address.
    ///
    /// If `version` is specified, the package loaded is the one that shares its original ID with the package at `address`, but whose version is `version`.
    ///
    /// If `atCheckpoint` is specified, the package loaded is the one with the largest version among all packages sharing an original ID with the package at `address` and was published at or before `atCheckpoint`.
    ///
    /// If neither are specified, the package is fetched at the latest checkpoint.
    ///
    /// It is an error to specify both `version` and `atCheckpoint`, and `null` will be returned if the package cannot be found as of the latest checkpoint, or the address points to an object that is not a package.
    ///
    /// Note that this interpretation of `version` and "latest" differs from the one used by `Query.object`, because non-system package upgrades generate objects with different IDs. To fetch a package using the versioning semantics of objects, use `Object.asMovePackage` nested under `Query.object`.
    async fn package(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<UInt53>,
        at_checkpoint: Option<UInt53>,
    ) -> Result<Option<MovePackage>, RpcError<move_package::Error>> {
        MovePackage::by_key(
            ctx,
            self.scope(ctx)?,
            PackageKey {
                address,
                version,
                at_checkpoint,
            },
        )
        .await
    }

    /// Fetch the protocol config by protocol version.
    async fn protocol_configs(&self, version: UInt53) -> Option<ProtocolConfigs> {
        Some(ProtocolConfigs::with_protocol_version(version.into()))
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
        Transaction::fetch(ctx, self.scope(ctx)?, digest).await
    }

    /// Fetch transaction effects by its transaction's digest.
    ///
    /// Returns `null` if the transaction effects do not exist in the store, either because that transaction was not executed, or it was pruned.
    async fn transaction_effects(
        &self,
        ctx: &Context<'_>,
        digest: Digest,
    ) -> Result<Option<TransactionEffects>, RpcError> {
        TransactionEffects::fetch(ctx, self.scope(ctx)?, digest).await
    }
}

impl Query {
    /// The scope under which all queries are supposed to be queried.
    fn scope<E: std::error::Error>(&self, ctx: &Context<'_>) -> Result<Scope, RpcError<E>> {
        self.scope.clone().map_or_else(|| Scope::new(ctx), Ok)
    }
}
