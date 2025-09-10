// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, InputObject, Interface, Object,
};
use diesel::{sql_types::Bool, ExpressionMethods, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use futures::future::try_join_all;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_reader::{
    consistent_reader::{self, ConsistentReader},
    kv_loader::KvLoader,
    object_versions::{
        CheckpointBoundedObjectVersionKey, VersionBoundedObjectVersionKey,
        VersionedObjectVersionKey,
    },
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};
use sui_pg_db::sql;
use sui_types::{
    base_types::{
        SequenceNumber, SuiAddress as NativeSuiAddress, TransactionDigest, VersionDigest,
    },
    digests::ObjectDigest,
    dynamic_field::DynamicFieldType,
    object::Object as NativeObject,
    transaction::GenesisObject,
};
use tokio::{join, sync::OnceCell};

use crate::{
    api::scalars::{
        base64::Base64,
        big_int::BigInt,
        cursor::{BcsCursor, JsonCursor},
        owner_kind::OwnerKind,
        sui_address::SuiAddress,
        type_filter::{TypeFilter, TypeInput},
        uint53::UInt53,
    },
    error::{bad_user_input, feature_unavailable, upcast, RpcError},
    intersect,
    pagination::{Page, PageLimits, PaginationConfig},
    scope::Scope,
};

use super::{
    address::Address,
    balance::{self, Balance},
    coin_metadata::CoinMetadata,
    dynamic_field::{DynamicField, DynamicFieldName},
    move_object::MoveObject,
    move_package::MovePackage,
    object_filter::{ObjectFilter, Validator as OFValidator},
    owner::Owner,
    transaction::{filter::TransactionFilter, CTransaction, Transaction},
};

/// Interface implemented by versioned on-chain values that are addressable by an ID (also referred to as its address). This includes Move objects and packages.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IObject",
    field(
        name = "version",
        ty = "Result<Option<UInt53>, RpcError>",
        desc = "The version of this object that this content comes from.",
    ),
    field(
        name = "digest",
        ty = "Result<Option<String>, RpcError>",
        desc = "32-byte hash that identifies the object's contents, encoded in Base58.",
    ),
    field(
        name = "object_at",
        arg(name = "version", ty = "Option<UInt53>"),
        arg(name = "root_version", ty = "Option<UInt53>"),
        arg(name = "checkpoint", ty = "Option<UInt53>"),
        ty = "Result<Option<Object>, RpcError<Error>>",
        desc = "Fetch the object with the same ID, at a different version, root version bound, or checkpoint.",
    ),
    field(
        name = "object_bcs",
        ty = "Result<Option<Base64>, RpcError>",
        desc = "The Base64-encoded BCS serialization of this object, as an `Object`."
    ),
    field(
        name = "object_versions_after",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<CVersion>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<CVersion>"),
        arg(name = "filter", ty = "Option<VersionFilter>"),
        ty = "Result<Option<Connection<String, Object>>, RpcError>",
        desc = "Paginate all versions of this object after this one."
    ),
    field(
        name = "object_versions_before",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<CVersion>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<CVersion>"),
        arg(name = "filter", ty = "Option<VersionFilter>"),
        ty = "Result<Option<Connection<String, Object>>, RpcError>",
        desc = "Paginate all versions of this object before this one."
    ),
    field(
        name = "owner",
        ty = "Result<Option<Owner>, RpcError<Error>>",
        desc = "The object's owner kind."
    ),
    field(
        name = "previous_transaction",
        ty = "Result<Option<Transaction>, RpcError<Error>>",
        desc = "The transaction that created this version of the object"
    ),
    field(
        name = "storage_rebate",
        ty = "Result<Option<BigInt>, RpcError<Error>>",
        desc = "The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object."
    ),
    field(
        name = "received_transactions",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<CTransaction>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<CTransaction>"),
        arg(name = "filter", ty = "Option<TransactionFilter>"),
        ty = "Result<Option<Connection<String, Transaction>>, RpcError>",
        desc = "The transactions that sent objects to this object."
    )
)]
pub(crate) enum IObject {
    CoinMetadata(CoinMetadata),
    DynamicField(DynamicField),
    MoveObject(MoveObject),
    MovePackage(MovePackage),
    Object(Object),
}

#[derive(Clone)]
pub(crate) struct Object {
    pub(crate) super_: Address,
    pub(crate) version_digest: Option<VersionDigest>,
    pub(crate) contents: Arc<OnceCell<Option<NativeObject>>>,
}

/// Identifies a specific version of an object.
///
/// The `address` field must be specified, as well as at most one of `version`, `rootVersion`, or `atCheckpoint`. If none are provided, the object is fetched at the current checkpoint.
///
/// Specifying a `version` or a `rootVersion` disables nested queries for paginating owned objects or dynamic fields (these queries are only supported at checkpoint boundaries).
///
/// See `Query.object` for more details.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    /// The object's ID.
    pub(crate) address: SuiAddress,

    /// If specified, tries to fetch the object at this exact version.
    pub(crate) version: Option<UInt53>,

    /// If specified, tries to fetch the latest version of the object at or before this version. Nested dynamic field accesses will also be subject to this bound.
    ///
    /// This can be used to fetch a child or ancestor object bounded by its root object's version. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    pub(crate) root_version: Option<UInt53>,

    /// If specified, tries to fetch the latest version as of this checkpoint. Fails if the checkpoint is later than the RPC's latest checkpoint.
    pub(crate) at_checkpoint: Option<UInt53>,
}

/// Filter for paginating the history of an Object or MovePackage.
#[derive(InputObject, Default, Debug)]
pub(crate) struct VersionFilter {
    /// Filter to versions that are strictly newer than this one, defaults to fetching from the earliest version known to this RPC (this could be the initial version, or some later version if the initial version has been pruned).
    pub(crate) after_version: Option<UInt53>,

    /// Filter to versions that are strictly older than this one, defaults to fetching up to the latest version (inclusive).
    pub(crate) before_version: Option<UInt53>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Cursors are pinned to different checkpoints: {0} vs {1}")]
    CursorInconsistency(u64, u64),

    #[error("At most one of a version, a root version, or a checkpoint bound can be specified when fetching an object")]
    OneBound,

    #[error("Request is outside consistent range")]
    OutOfRange(u64),

    #[error("Checkpoint {0} in the future")]
    Future(u64),

    #[error("Cannot paginate owned objects for a parent object's address if its version is bounded. Fetch the parent at a checkpoint in the consistent range to list its owned objects.")]
    RootVersionOwnership,
}

pub(crate) type CLive = BcsCursor<(u64, Vec<u8>)>;
pub(crate) type CVersion = JsonCursor<u64>;

/// An Object on Sui is either a typed value (a Move Object) or a Package (modules containing functions and types).
///
/// Every object on Sui is identified by a unique address, and has a version number that increases with every modification. Objects also hold metadata detailing their current owner (who can sign for access to the object and whether that access can modify and/or delete the object), and the digest of the last transaction that modified the object.
#[Object]
impl Object {
    /// The Object's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        if let Some((version, _)) = self.version_digest {
            return Ok(Some(version.into()));
        }

        // Fall back to loading from contents
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(contents.version().into()))
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        if let Some((_, digest)) = self.version_digest {
            return Ok(Some(Base58::encode(digest.inner())));
        }

        // Fall back to loading from contents
        let Some(contents) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(Base58::encode(contents.digest().inner())))
    }

    /// Attempts to convert the object into a MoveObject.
    async fn as_move_object(&self, ctx: &Context<'_>) -> Result<Option<MoveObject>, RpcError> {
        MoveObject::from_object(self, ctx).await
    }

    /// Attempts to convert the object into a MovePackage.
    async fn as_move_package(&self, ctx: &Context<'_>) -> Result<Option<MovePackage>, RpcError> {
        MovePackage::from_object(self, ctx).await
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<Balance>, RpcError<balance::Error>> {
        self.super_.balance(ctx, coin_type).await
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Option<Connection<String, Balance>>, RpcError<balance::Error>> {
        self.super_.balances(ctx, first, after, last, before).await
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<String>, RpcError> {
        self.super_.default_suins_name(ctx).await
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<Error>> {
        DynamicField::by_name(
            ctx,
            self.super_.scope.clone(),
            self.super_.address.into(),
            DynamicFieldType::DynamicField,
            name,
        )
        .await
        .map_err(upcast)
    }

    /// Dynamic fields owned by this object.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
    ) -> Result<Option<Connection<String, DynamicField>>, RpcError<Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("Object", "dynamicFields");
        let page = Page::from_params(limits, first, after, last, before)?;

        let dynamic_fields = DynamicField::paginate(
            ctx,
            self.super_.scope.clone(),
            self.super_.address.into(),
            page,
        )
        .await?;

        Ok(Some(dynamic_fields))
    }

    /// Access a dynamic object field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic object field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<Error>> {
        DynamicField::by_name(
            ctx,
            self.super_.scope.clone(),
            self.super_.address.into(),
            DynamicFieldType::DynamicObject,
            name,
        )
        .await
        .map_err(upcast)
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<Error>> {
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                self.super_.scope.clone(),
                self.super_.address.into(),
                DynamicFieldType::DynamicField,
                key,
            )
        }))
        .await
        .map_err(upcast)
    }

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<Error>> {
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                self.super_.scope.clone(),
                self.super_.address.into(),
                DynamicFieldType::DynamicObject,
                key,
            )
        }))
        .await
        .map_err(upcast)
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Option<Vec<Balance>>, RpcError<balance::Error>> {
        self.super_.multi_get_balances(ctx, keys).await
    }

    /// Fetch the object with the same ID, at a different version, root version bound, or checkpoint.
    ///
    /// If no additional bound is provided, the latest version of this object is fetched at the latest checkpoint.
    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let key = ObjectKey {
            address: self.super_.address.into(),
            version,
            root_version,
            at_checkpoint: checkpoint,
        };

        Object::by_key(ctx, self.super_.scope.without_root_version(), key).await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        let Some(object) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(object).context("Failed to serialize object")?;
        Ok(Some(Base64(bytes)))
    }

    /// Paginate all versions of this object after this one.
    pub(crate) async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IObject", "objectVersionsAfter");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(version) = self.version(ctx).await? else {
            return Ok(None);
        };

        // Apply any filter that was supplied to the query, but add an additional version
        // lowerbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            after_version: Some(version),
            ..VersionFilter::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Ok(Some(
            Object::paginate_by_version(
                ctx,
                self.super_.scope.without_root_version(),
                page,
                self.super_.address,
                filter,
            )
            .await?,
        ))
    }

    /// Paginate all versions of this object before this one.
    pub(crate) async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IObject", "objectVersionsBefore");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(version) = self.version(ctx).await? else {
            return Ok(None);
        };

        // Apply any filter that was supplied to the query, but add an additional version
        // upperbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            before_version: Some(version),
            ..VersionFilter::default()
        }) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Ok(Some(
            Object::paginate_by_version(
                ctx,
                self.super_.scope.without_root_version(),
                page,
                self.super_.address,
                filter,
            )
            .await?,
        ))
    }

    /// Objects owned by this object, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<Error>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Result<Option<Owner>, RpcError> {
        let Some(object) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(Owner::from_native(
            self.super_.scope.clone(),
            object.owner.clone(),
        )))
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError> {
        let Some(object) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(Transaction::with_id(
            self.super_.scope.without_root_version(),
            object.previous_transaction,
        )))
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<BigInt>, RpcError> {
        let Some(object) = self.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        Ok(Some(BigInt::from(object.storage_rebate)))
    }

    /// The transactions that sent objects to this object
    pub(crate) async fn received_transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        filter: Option<TransactionFilter>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IObject", "receivedTransactions");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Create filter for transactions that affected this object's address
        let address_filter = TransactionFilter {
            affected_address: Some(self.super_.address.into()),
            ..Default::default()
        };

        // Intersect with user-provided filter
        let Some(filter) = filter.unwrap_or_default().intersect(address_filter) else {
            return Ok(Some(Connection::new(false, false)));
        };

        Transaction::paginate(ctx, self.super_.scope.clone(), page, filter)
            .await
            .map(Some)
    }
}

impl Object {
    /// Construct an object that is represented by just its identifier (its object reference). This
    /// does not check whether the object exists, so should not be used to "fetch" an object based
    /// on an address and/or version provided as user input.
    pub(crate) fn with_ref(
        scope: &Scope,
        address: NativeSuiAddress,
        version: SequenceNumber,
        digest: ObjectDigest,
    ) -> Self {
        // Set root_version since we're creating an object at a specific version
        let scope = scope.with_root_version(version.into());
        let super_ = Address::with_address(scope, address);

        Self {
            super_,
            version_digest: Some((version, digest)),
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Construct an object that is represented by just its address. This does not check that the
    /// object exists, so should not be used to "fetch" an address provided as user input. When the
    /// object's contents are fetched from the latest version of that object as of the current
    /// checkpoint.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        Self {
            super_: Address::with_address(scope, address),
            version_digest: None,
            contents: Arc::new(OnceCell::new()),
        }
    }

    /// Fetch an object by its key. The key can either specify an exact version to fetch, an
    /// upperbound against a "root version", an upperbound against a checkpoint, or none of the
    /// above. Returns `None` when no checkpoint is set in scope (e.g. execution scope)
    /// and no explicit version is provided.
    pub(crate) async fn by_key(
        ctx: &Context<'_>,
        scope: Scope,
        key: ObjectKey,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let bounds = key.version.is_some() as u8
            + key.root_version.is_some() as u8
            + key.at_checkpoint.is_some() as u8;

        if bounds > 1 {
            Err(bad_user_input(Error::OneBound))
        } else if let Some(v) = key.version {
            Self::at_version(ctx, scope, key.address, v)
                .await
                .map_err(upcast)
        } else if let Some(v) = key.root_version {
            Self::version_bounded(ctx, scope, key.address, v)
                .await
                .map_err(upcast)
        } else if let Some(cp) = key.at_checkpoint {
            let scope = scope
                .with_checkpoint_viewed_at(cp.into())
                .ok_or_else(|| bad_user_input(Error::Future(cp.into())))?;

            Self::checkpoint_bounded(ctx, scope, key.address, cp)
                .await
                .map_err(upcast)
        } else {
            Self::latest(ctx, scope, key.address).await.map_err(upcast)
        }
    }

    /// Fetch the latest version of the object at the given address less than or equal to
    /// `root_version`.
    pub(crate) async fn version_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        root_version: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let Some(stored) = pg_loader
            .load_one(VersionBoundedObjectVersionKey(
                address.into(),
                root_version.into(),
            ))
            .await
            .context("Failed to fetch object versions")?
        else {
            return Ok(None);
        };

        Object::from_stored_version(scope.with_root_version(root_version.into()), stored)
    }

    /// Get the latest version of the object at the given address, as of the latest checkpoint
    /// according to `scope`.
    pub(crate) async fn latest(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
    ) -> Result<Option<Self>, RpcError> {
        let Some(cp) = scope.checkpoint_viewed_at() else {
            return Ok(None);
        };

        Self::checkpoint_bounded(ctx, scope, address, cp.into()).await
    }

    /// Fetch the latest version of the object at the given address as of the checkpoint with
    /// sequence number `at_checkpoint`.
    pub(crate) async fn checkpoint_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        at_checkpoint: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let Some(stored) = pg_loader
            .load_one(CheckpointBoundedObjectVersionKey(
                address.into(),
                at_checkpoint.into(),
            ))
            .await
            .context("Failed to fetch object versions")?
        else {
            return Ok(None);
        };

        Object::from_stored_version(scope, stored)
    }

    /// Load the object at the given ID and version from the store, and return it fully inflated
    /// (with contents already fetched). Returns `None` if the object does not exist (either never
    /// existed, was pruned from the store, or did not exist at the checkpoint being viewed).
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn at_version(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(None);
        };

        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let kv_loader: &KvLoader = ctx.data()?;

        let contents = kv_loader.load_one_object(address.into(), version.into());
        let stored_version =
            pg_loader.load_one(VersionedObjectVersionKey(address.into(), version.into()));
        let (contents, stored_version) = join!(contents, stored_version);

        let Some(c) = contents.context("Failed to fetch object contents")? else {
            return Ok(None);
        };

        if stored_version
            .context("Failed to get object version")?
            .is_none_or(|s| s.cp_sequence_number as u64 > checkpoint_viewed_at)
        {
            return Ok(None);
        }

        Ok(Some(Self::from_contents(
            scope.with_root_version(version.into()),
            c,
        )))
    }

    /// Construct a GraphQL representation of an `Object` from a raw object bundled into the genesis transaction.
    pub(crate) fn from_genesis_object(scope: Scope, genesis_obj: GenesisObject) -> Self {
        let GenesisObject::RawObject { data, owner } = genesis_obj;
        let prev = TransactionDigest::genesis_marker();
        let native = NativeObject::new_from_genesis(data, owner, prev);

        Self::from_contents(scope, native)
    }

    /// Construct a GraphQL representation of an `Object` from its native representation.
    ///
    /// Note that this constructor does not adjust version bounds in the scope. It is the
    /// caller's responsibility to do that, if appropriate.
    pub(crate) fn from_contents(scope: Scope, contents: NativeObject) -> Self {
        let address = Address::with_address(scope, contents.id().into());

        Self {
            super_: address,
            version_digest: Some((contents.version(), contents.digest())),
            contents: Arc::new(OnceCell::from(Some(contents))),
        }
    }

    /// Construct a GraphQL representation of an `Object` from versioning information. This
    /// representation does not pre-fetch object contents.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    fn from_stored_version(
        scope: Scope,
        stored: StoredObjVersion,
    ) -> Result<Option<Self>, RpcError> {
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(None);
        };

        // Lack of an object digest indicates that the object was deleted or wrapped at this
        // version.
        let Some(digest) = stored.object_digest else {
            return Ok(None);
        };

        // If the object's version is from a later checkpoint than is being viewed currently, then
        // discard this result.
        if stored.cp_sequence_number as u64 > checkpoint_viewed_at {
            return Ok(None);
        }

        let address = Address::with_address(
            scope,
            NativeSuiAddress::from_bytes(stored.object_id)
                .context("Failed to deserialize SuiAddress")?,
        );

        Ok(Some(Object {
            super_: address,
            version_digest: Some((
                SequenceNumber::from_u64(stored.object_version as u64),
                ObjectDigest::try_from(&digest[..])
                    .context("Failed to deserialize Object Digest")?,
            )),
            contents: Arc::new(OnceCell::new()),
        }))
    }

    /// Paginate through versions of an object (identified by its address).
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate_by_version(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CVersion>,
        address: NativeSuiAddress,
        filter: VersionFilter,
    ) -> Result<Connection<String, Object>, RpcError> {
        use obj_versions::dsl as v;

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let mut conn = Connection::new(false, false);

        let pg_reader: &PgReader = ctx.data()?;

        let mut query = v::obj_versions
            .filter(v::object_id.eq(address.to_vec()))
            .filter(sql!(as Bool,
                r#"
                    object_version <= (SELECT
                        m.object_version
                    FROM
                        obj_versions m
                    WHERE
                        m.object_id = obj_versions.object_id
                    AND m.cp_sequence_number <= {BigInt}
                    ORDER BY
                        m.cp_sequence_number DESC,
                        m.object_version DESC
                    LIMIT 1)
                "#,
                checkpoint_viewed_at as i64,
            ))
            .limit(page.limit() as i64 + 2)
            .into_boxed();

        if let Some(after_version) = filter.after_version {
            query = query.filter(v::object_version.gt(i64::from(after_version)));
        }

        if let Some(before_version) = filter.before_version {
            query = query.filter(v::object_version.lt(i64::from(before_version)));
        }

        query = if page.is_from_front() {
            query.order_by(v::object_version)
        } else {
            query.order_by(v::object_version.desc())
        };

        if let Some(after) = page.after() {
            query = query.filter(v::object_version.ge(**after as i64));
        }

        if let Some(before) = page.before() {
            query = query.filter(v::object_version.le(**before as i64));
        }

        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let mut results: Vec<StoredObjVersion> = c
            .results(query)
            .await
            .context("Failed to read from database")?;

        if !page.is_from_front() {
            results.reverse();
        }

        let (prev, next, results) =
            page.paginate_results(results, |v| JsonCursor::new(v.object_version as u64));

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for (cursor, stored) in results {
            let scope = scope.with_root_version(stored.object_version as u64);
            if let Some(object) = Self::from_stored_version(scope, stored)? {
                conn.edges.push(Edge::new(cursor.encode_cursor(), object));
            }
        }

        Ok(conn)
    }

    /// Paginate through objects in the live object set.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate_live(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CLive>,
        filter: ObjectFilter,
    ) -> Result<Connection<String, Object>, RpcError<Error>> {
        if scope.root_version().is_some() {
            return Err(bad_user_input(Error::RootVersionOwnership));
        }
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let consistent_reader: &ConsistentReader = ctx.data()?;

        // Figure out which checkpoint to pin results to, based on the pagination cursors and
        // defaulting to the current scope. If both cursors are provided, they must agree on the
        // checkpoint they are pinning, and this checkpoint must be at or below the scope's latest
        // checkpoint.
        let checkpoint = match (page.after(), page.before()) {
            (Some(a), Some(b)) if a.0 != b.0 => {
                return Err(bad_user_input(Error::CursorInconsistency(a.0, b.0)));
            }
            (None, None) => checkpoint_viewed_at,
            (Some(c), _) | (_, Some(c)) => c.0,
        };

        // Set the checkpoint being viewed to the one calculated from the cursors, so that
        // nested queries about the resulting objects also treat this checkpoint as latest.
        let Some(scope) = scope.with_checkpoint_viewed_at(checkpoint) else {
            return Err(bad_user_input(Error::Future(checkpoint)));
        };

        let refs = match filter {
            ObjectFilter {
                owner_kind: kind @ (None | Some(OwnerKind::Address | OwnerKind::Object)),
                owner: Some(address),
                type_,
            } => {
                consistent_reader
                    .list_owned_objects(
                        checkpoint,
                        kind.unwrap_or(OwnerKind::Address).into(),
                        Some(address.to_string()),
                        type_.map(|t| t.to_string()),
                        Some(page.limit() as u32),
                        page.after().map(|c| c.1.clone()),
                        page.before().map(|c| c.1.clone()),
                        page.is_from_front(),
                    )
                    .await
            }

            ObjectFilter {
                owner_kind: Some(kind @ (OwnerKind::Shared | OwnerKind::Immutable)),
                owner: None,
                type_: Some(type_),
            } => {
                consistent_reader
                    .list_owned_objects(
                        checkpoint,
                        kind.into(),
                        None,
                        Some(type_.to_string()),
                        Some(page.limit() as u32),
                        page.after().map(|c| c.1.clone()),
                        page.before().map(|c| c.1.clone()),
                        page.is_from_front(),
                    )
                    .await
            }

            ObjectFilter {
                owner_kind: None,
                owner: None,
                type_: Some(type_),
            } => {
                consistent_reader
                    .list_objects_by_type(
                        checkpoint,
                        type_.to_string(),
                        Some(page.limit() as u32),
                        page.after().map(|c| c.1.clone()),
                        page.before().map(|c| c.1.clone()),
                        page.is_from_front(),
                    )
                    .await
            }
            _ => {
                return Err(
                    anyhow!("Invalid ObjectFilter not caught by validation: {filter:?}").into(),
                )
            }
        }
        .map_err(|e| match e {
            consistent_reader::Error::NotConfigured => {
                feature_unavailable("paginating the live object set")
            }

            consistent_reader::Error::OutOfRange(_) => {
                bad_user_input(Error::OutOfRange(checkpoint))
            }

            consistent_reader::Error::Internal(error) => {
                error.context("Failed to fetch live objects").into()
            }
        })?;

        let mut conn = Connection::new(false, false);
        if refs.results.is_empty() {
            return Ok(conn);
        }

        conn.has_previous_page = refs.has_previous_page;
        conn.has_next_page = refs.has_next_page;

        for edge in refs.results {
            let (id, version, digest) = edge.value;

            let cursor = CLive::new((checkpoint, edge.token));
            let address = Address::with_address(scope.clone(), id.into());
            let object = Object {
                super_: address,
                version_digest: Some((version, digest)),
                contents: Arc::new(OnceCell::new()),
            };
            conn.edges.push(Edge::new(cursor.encode_cursor(), object));
        }

        Ok(conn)
    }

    /// Fetch a singleton object of a given type, assuming there is at most one live object of that
    /// type in the live object set.
    ///
    /// Returns `None` if there is no live object of the given type.
    pub(crate) async fn singleton(
        ctx: &Context<'_>,
        scope: Scope,
        type_: StructTag,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let filter = ObjectFilter {
            type_: Some(TypeFilter::Type(type_)),
            owner_kind: None,
            owner: None,
        };

        // Query for objects of this type with a limit of 1
        let page = Page::from_params(&PageLimits::singleton(), Some(1), None, None, None)?;
        let mut connection = Self::paginate_live(ctx, scope, page, filter).await?;

        // Get the first (and should be only) result
        Ok(connection.edges.pop().map(|edge| edge.node))
    }

    /// Return the object's contents, lazily loading it if necessary.
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<NativeObject>, RpcError> {
        self.contents
            .get_or_try_init(async || {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
                let kv_loader: &KvLoader = ctx.data()?;

                let version = if let Some((version, _)) = self.version_digest {
                    version.into()
                } else if let Some(cp) = self.super_.scope.checkpoint_viewed_at() {
                    // If we don't have a version, we need to do a checkpoint-bounded lookup first
                    // to get the version, then fetch contents with that version.
                    let Some(stored) = pg_loader
                        .load_one(CheckpointBoundedObjectVersionKey(
                            self.super_.address.into(),
                            cp,
                        ))
                        .await
                        .context("Failed to fetch object version")?
                    else {
                        return Ok(None);
                    };

                    UInt53::from(stored.object_version as u64)
                } else {
                    return Ok(None);
                };

                // Check execution context cache first and return if available
                if let Some(cached_object) = self
                    .super_
                    .scope
                    .execution_output_object(self.super_.address.into(), version.into())
                {
                    Ok(Some(cached_object.clone()))
                } else {
                    Ok(kv_loader
                        .load_one_object(self.super_.address.into(), version.into())
                        .await
                        .context("Failed to fetch object contents")?)
                }
            })
            .await
    }
}

impl VersionFilter {
    /// Try to create a filter whose results are the intersection of `self`'s results and `other`'s
    /// results. This may not be possible if the resulting filter is inconsistent (guaranteed to
    /// produce no results).
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        let a = intersect::field(self.after_version, other.after_version, intersect::by_max)?;
        let b = intersect::field(self.before_version, other.before_version, intersect::by_min)?;

        match (a.map(u64::from), b.map(u64::from)) {
            // There are no versions strictly before version 0
            (_, Some(0)) => None,

            // If `before` is not at least two away from `after`, the interval is empty
            (Some(a), Some(b)) if b.saturating_sub(a) <= 1 => None,

            _ => Some(Self {
                after_version: a,
                before_version: b,
            }),
        }
    }
}
