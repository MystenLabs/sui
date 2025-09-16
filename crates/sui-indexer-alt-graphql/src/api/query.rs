// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_graphql::{connection::Connection, Context, Object, Result};
use futures::future::try_join_all;
use sui_indexer_alt_reader::fullnode_client::{Error::GrpcExecutionError, FullnodeClient};
use sui_types::{digests::ChainIdentifier, transaction::TransactionData};

use crate::{
    api::{
        mutation::TransactionInputError,
        scalars::base64::Base64,
        types::{
            epoch::CEpoch, simulation_result::SimulationResult,
            transaction_effects::TransactionEffects,
        },
    },
    error::{bad_user_input, upcast, RpcError},
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    scalars::{
        digest::Digest, domain::Domain, sui_address::SuiAddress, type_filter::TypeInput,
        uint53::UInt53,
    },
    types::{
        address::Address,
        checkpoint::{filter::CheckpointFilter, CCheckpoint, Checkpoint},
        coin_metadata::CoinMetadata,
        epoch::Epoch,
        event::{filter::EventFilter, CEvent, Event},
        move_package::{self, MovePackage, PackageCheckpointFilter, PackageKey},
        move_type::{self, MoveType},
        name_service::name_to_address,
        object::{self, Object, ObjectKey, VersionFilter},
        object_filter::{ObjectFilter, ObjectFilterValidator as OFValidator},
        protocol_configs::ProtocolConfigs,
        service_config::ServiceConfig,
        transaction::{
            filter::{TransactionFilter, TransactionFilterValidator as TFValidator},
            CTransaction, Transaction,
        },
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
    /// Look-up an account by its SuiAddress.
    ///
    /// If `rootVersion` is specified, nested dynamic field accesses will be fetched at or before this version. This can be used to fetch a child or ancestor object bounded by its root object's version, when its immediate parent is wrapped, or a value in a dynamic object field. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    ///
    /// Specifying a `rootVersion` disables nested queries for paginating owned objects or dynamic fields (these queries are only supported at checkpoint boundaries).
    async fn address(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        root_version: Option<UInt53>,
    ) -> Result<Address, RpcError> {
        let mut scope = self.scope(ctx)?;
        if let Some(version) = root_version {
            scope = scope.with_root_version(version.into());
        }

        Ok(Address::with_address(scope, address.into()))
    }

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
        Ok(Checkpoint::with_sequence_number(
            scope,
            sequence_number.map(|s| s.into()),
        ))
    }

    /// Paginate checkpoints in the network, optionally bounded to checkpoints in the given epoch.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CCheckpoint>,
        last: Option<u64>,
        before: Option<CCheckpoint>,
        filter: Option<CheckpointFilter>,
    ) -> Result<Connection<String, Checkpoint>, RpcError> {
        let scope = self.scope(ctx)?;
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "checkpoints");
        let page = Page::from_params(limits, first, after, last, before)?;

        let filter = filter.unwrap_or_default();

        Checkpoint::paginate(ctx, scope, page, filter).await
    }

    /// Fetch the CoinMetadata for a given coin type.
    ///
    /// Returns `null` if no CoinMetadata object exists for the given coin type.
    async fn coin_metadata(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<CoinMetadata>, RpcError<object::Error>> {
        CoinMetadata::by_coin_type(ctx, self.scope(ctx)?, coin_type.into()).await
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

    /// Paginate epochs that are in the network.
    async fn epochs(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEpoch>,
        last: Option<u64>,
        before: Option<CEpoch>,
    ) -> Result<Option<Connection<String, Epoch>>, RpcError> {
        let scope = self.scope(ctx)?;
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "epochs");
        let page = Page::from_params(limits, first, after, last, before)?;

        Epoch::paginate(ctx, &scope, page).await
    }

    /// Paginate events that are emitted in the network, optionally filtered by event filters.
    async fn events(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CEvent>,
        last: Option<u64>,
        before: Option<CEvent>,
        filter: Option<EventFilter>,
    ) -> Result<Connection<String, Event>, RpcError> {
        let scope = self.scope(ctx)?;
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "events");
        let page = Page::from_params(limits, first, after, last, before)?;

        Event::paginate(ctx, scope, page, filter.unwrap_or_default()).await
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
            .map(|k| Checkpoint::with_sequence_number(scope.clone(), Some(k.into())))
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

    /// Fetch types by their string representations.
    ///
    /// Types are canonicalized: In the input they can be at any package address at or after the package that first defines them, and in the output they will be relocated to the package that first defines them.
    ///
    /// Returns a list of types that is guaranteed to be the same length as `keys`. If a type in `keys` could not be found, its corresponding entry in the result will be `null`.
    async fn multi_get_types(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Vec<Option<MoveType>>, RpcError<move_type::Error>> {
        let types = keys
            .into_iter()
            .map(|t| async move { MoveType::canonicalize(t.into(), self.scope(ctx)?).await });

        try_join_all(types).await
    }

    /// Fetch an object by its address.
    ///
    /// If `version` is specified, the object will be fetched at that exact version.
    ///
    /// If `rootVersion` is specified, the object will be fetched at the latest version at or before this version. Nested dynamic field accesses will also be subject to this bound. This can be used to fetch a child or ancestor object bounded by its root object's version. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    ///
    /// Specifying a `version` or a `rootVersion` disables nested queries for paginating owned objects or dynamic fields (these queries are only supported at checkpoint boundaries).
    ///
    /// If `atCheckpoint` is specified, the object will be fetched at the latest version as of this checkpoint. This will fail if the provided checkpoint is after the RPC's latest checkpoint.
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

    /// Paginate objects in the live object set, optionally filtered by owner and/or type. `filter` can be one of:
    ///
    /// - A filter on type (all live objects whose type matches that filter).
    /// - Fetching all objects owned by an address or object, optionally filtered by type.
    /// - Fetching all shared or immutable objects, filtered by type.
    async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CLive>,
        last: Option<u64>,
        before: Option<object::CLive>,
        #[graphql(validator(custom = "OFValidator::default()"))] filter: ObjectFilter,
    ) -> Result<Option<Connection<String, Object>>, RpcError<object::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "objects");
        let page = Page::from_params(limits, first, after, last, before)?;

        Ok(Some(
            Object::paginate_live(ctx, self.scope(ctx)?, page, filter).await?,
        ))
    }

    /// Paginate all versions of an object at `address`, optionally bounding the versions exclusively from below with `filter.afterVersion` or from above with `filter.beforeVersion`.
    async fn object_versions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CVersion>,
        last: Option<u64>,
        before: Option<object::CVersion>,
        address: SuiAddress,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "objectVersions");
        let page = Page::from_params(limits, first, after, last, before)?;

        Ok(Some(
            Object::paginate_by_version(
                ctx,
                self.scope(ctx)?,
                page,
                address.into(),
                filter.unwrap_or_default(),
            )
            .await?,
        ))
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

    /// Paginate all packages published on-chain, optionally bounded to packages published strictly after `filter.afterCheckpoint` and/or strictly before `filter.beforeCheckpoint`.
    async fn packages(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<move_package::CPackage>,
        last: Option<u64>,
        before: Option<move_package::CPackage>,
        filter: Option<PackageCheckpointFilter>,
    ) -> Result<Option<Connection<String, MovePackage>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "packages");
        let page = Page::from_params(limits, first, after, last, before)?;

        Ok(Some(
            MovePackage::paginate_by_checkpoint(
                ctx,
                self.scope(ctx)?,
                page,
                filter.unwrap_or_default(),
            )
            .await?,
        ))
    }

    /// Paginate all versions of a package at `address`, optionally bounding the versions exclusively from below with `filter.afterVersion` or from above with `filter.beforeVersion`.
    ///
    /// Different versions of a package will have different object IDs, unless they are system packages, but will share the same original ID.
    async fn package_versions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::CVersion>,
        last: Option<u64>,
        before: Option<object::CVersion>,
        address: SuiAddress,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, MovePackage>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "packageVersions");
        let page = Page::from_params(limits, first, after, last, before)?;

        Ok(Some(
            MovePackage::paginate_by_version(
                ctx,
                self.scope(ctx)?,
                page,
                address.into(),
                filter.unwrap_or_default(),
            )
            .await?,
        ))
    }

    /// Fetch the protocol config by protocol version, or the latest protocol config used on chain if no version is provided.
    async fn protocol_configs(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
    ) -> Result<Option<ProtocolConfigs>, RpcError> {
        if let Some(version) = version {
            Ok(Some(ProtocolConfigs::with_protocol_version(version.into())))
        } else {
            let scope = self.scope(ctx)?;
            ProtocolConfigs::latest(ctx, &scope).await
        }
    }

    /// Configuration for this RPC service.
    async fn service_config(&self) -> ServiceConfig {
        ServiceConfig
    }

    /// Look-up an account by its SuiNS name, assuming it has a valid, unexpired name registration.
    async fn suins_name(
        &self,
        ctx: &Context<'_>,
        address: Domain,
        root_version: Option<UInt53>,
    ) -> Result<Option<Address>, RpcError> {
        let mut scope = self.scope(ctx)?;
        if let Some(version) = root_version {
            scope = scope.with_root_version(version.into());
        }

        name_to_address(ctx, &scope, &address).await
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

    /// The transactions that exist in the network, optionally filtered by transaction filters.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        #[graphql(validator(custom = "TFValidator"))] filter: Option<TransactionFilter>,
    ) -> Result<Connection<String, Transaction>, RpcError> {
        let scope = self.scope(ctx)?;
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Query", "transactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Use the filter if provided, otherwise use default (unfiltered)
        let filter = filter.unwrap_or_default();

        Transaction::paginate(ctx, scope, page, filter).await
    }

    /// Fetch a structured representation of a concrete type, including its layout information.
    ///
    /// Types are canonicalized: In the input they can be at any package address at or after the package that first defines them, and in the output they will be relocated to the package that first defines them.
    ///
    /// Fails if the type is malformed, returns `null` if a type mentioned does not exist.
    async fn type_(
        &self,
        ctx: &Context<'_>,
        type_: TypeInput,
    ) -> Result<Option<MoveType>, RpcError<move_type::Error>> {
        MoveType::canonicalize(type_.into(), self.scope(ctx)?).await
    }

    /// Simulate a transaction to preview its effects without executing it on chain.
    ///
    /// - `transactionDataBcs` contains the BCS-encoded transaction data (Base64-encoded).
    ///
    /// Unlike `executeTransaction`, this does not require signatures since the transaction is not committed to the blockchain. This allows for previewing transaction effects, estimating gas costs, and testing transaction logic without spending gas or requiring valid signatures.
    async fn simulate_transaction(
        &self,
        ctx: &Context<'_>,
        transaction_data_bcs: Base64,
    ) -> Result<SimulationResult, RpcError<TransactionInputError>> {
        let fullnode_client: &FullnodeClient = ctx.data()?;

        // Parse transaction data from BCS
        let tx_data: TransactionData = {
            let bytes: &Vec<u8> = &transaction_data_bcs.0;
            bcs::from_bytes(bytes)
                .map_err(|err| bad_user_input(TransactionInputError::InvalidTransactionBcs(err)))?
        };

        // Simulate transaction - no signatures needed
        match fullnode_client.simulate_transaction(tx_data.clone()).await {
            Ok(response) => {
                let scope = self.scope(ctx)?;
                SimulationResult::from_simulation_response(scope, response, tx_data).map_err(upcast)
            }
            Err(GrpcExecutionError(status)) => Ok(SimulationResult {
                effects: None,
                events: None,
                outputs: None,
                error: Some(status.to_string()),
            }),
            Err(other_error) => Err(anyhow!(other_error)
                .context("Failed to simulate transaction")
                .into()),
        }
    }
}

impl Query {
    /// The scope under which all queries are supposed to be queried.
    fn scope<E: std::error::Error>(&self, ctx: &Context<'_>) -> Result<Scope, RpcError<E>> {
        self.scope.clone().map_or_else(|| Scope::new(ctx), Ok)
    }
}
