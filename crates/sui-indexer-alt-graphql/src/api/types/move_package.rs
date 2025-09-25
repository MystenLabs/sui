// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    dataloader::DataLoader,
    Context, InputObject, Object,
};
use diesel::{sql_types::Bool, ExpressionMethods, QueryDsl};
use serde::{Deserialize, Serialize};
use sui_indexer_alt_reader::{
    packages::{
        CheckpointBoundedOriginalPackageKey, PackageOriginalIdKey, VersionedOriginalPackageKey,
    },
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::{packages::StoredPackage, schema::kv_packages};
use sui_package_resolver::Package as ParsedMovePackage;
use sui_pg_db::sql;
use sui_sql_macro::query;
use sui_types::{
    base_types::{ObjectID, SuiAddress as NativeSuiAddress},
    move_package::MovePackage as NativeMovePackage,
    object::Object as NativeObject,
};
use tokio::sync::OnceCell;

use crate::{
    api::scalars::{
        base64::Base64,
        big_int::BigInt,
        cursor::{BcsCursor, JsonCursor},
        sui_address::SuiAddress,
        type_filter::TypeInput,
        uint53::UInt53,
    },
    error::{bad_user_input, upcast, RpcError},
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    balance::{self, Balance},
    linkage::Linkage,
    move_module::MoveModule,
    move_object::MoveObject,
    object::{self, CLive, CVersion, Object, VersionFilter},
    object_filter::{ObjectFilter, ObjectFilterValidator as OFValidator},
    owner::Owner,
    transaction::{filter::TransactionFilter, CTransaction, Transaction},
    type_origin::TypeOrigin,
};

#[derive(Clone)]
pub(crate) struct MovePackage {
    /// Representation of this Move Package as a generic Object.
    super_: Object,

    /// Move package specific data, lazily loaded from the super object.
    native: Arc<OnceCell<Option<NativeMovePackage>>>,

    /// In-memory indices that help find components of the package quickly.
    parsed: Arc<OnceCell<Option<ParsedMovePackage>>>,
}

/// Identifies a specific version of a package.
///
/// The `address` field must be specified, as well as at most one of `version`, or `atCheckpoint`. If neither is provided, the package is fetched at the current checkpoint.
///
/// See `Query.package` for more details.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct PackageKey {
    /// The object's ID.
    pub(crate) address: SuiAddress,

    /// If specified, tries to fetch the package at this exact version.
    pub(crate) version: Option<UInt53>,

    /// If specified, tries to fetch the latest version as of this checkpoint.
    pub(crate) at_checkpoint: Option<UInt53>,
}

/// Filter for paginating packages published within a range of checkpoints.
#[derive(InputObject, Default, Debug)]
pub(crate) struct PackageCheckpointFilter {
    /// Filter to packages that were published strictly after this checkpoint, defaults to fetching from the earliest checkpoint known to this RPC (this could be the genesis checkpoint, or some later checkpoint if data has been pruned).
    pub(crate) after_checkpoint: Option<UInt53>,

    /// Filter to packages published strictly before this checkpoint, defaults to fetching up to the latest checkpoint (inclusive).
    pub(crate) before_checkpoint: Option<UInt53>,
}

/// Inner struct for the cursor produced while iterating over all package publishes.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub(crate) struct PackageCursor {
    pub cp_sequence_number: u64,
    pub original_id: Vec<u8>,
    pub package_version: u64,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Checkpoint {0} in the future")]
    Future(u64),

    #[error(
        "At most one of a version, or a checkpoint bound can be specified when fetching a package"
    )]
    OneBound,
}

/// Cursor for iterating over modules in a package. Points to the module by its name.
pub(crate) type CModule = JsonCursor<String>;

/// Cursor for iterating over package publishes. Points to the publish of a particular
/// version of a package, in a given checkpoint.
pub(crate) type CPackage = BcsCursor<PackageCursor>;

/// Cursor for iterating over system packages. Points at a particular system package, by its ID.
pub(crate) type CSysPackage = BcsCursor<Vec<u8>>;

/// A MovePackage is a kind of Object that represents code that has been published on-chain. It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
    /// The MovePackage's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// The version of this package that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        self.super_.version(ctx).await
    }

    /// 32-byte hash that identifies the package's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        self.super_.digest(ctx).await
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

    /// The module named `name` in this package.
    async fn module(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Option<MoveModule>, RpcError> {
        let Some(parsed) = self.parsed(ctx).await?.as_ref() else {
            return Ok(None);
        };

        if parsed.module(&name).is_err() {
            return Ok(None);
        }

        Ok(Some(MoveModule::with_fq_name(self.clone(), name)))
    }

    /// Paginate through this package's modules.
    async fn modules(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CModule>,
        last: Option<u64>,
        before: Option<CModule>,
    ) -> Result<Option<Connection<String, MoveModule>>, RpcError> {
        use std::ops::Bound as B;

        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MovePackage", "modules");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(parsed) = self.parsed(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let module_range = parsed
            .modules()
            .range::<String, _>((
                page.after().map_or(B::Unbounded, |a| B::Excluded(&**a)),
                page.before().map_or(B::Unbounded, |b| B::Excluded(&**b)),
            ))
            .map(|(name, _)| name.clone());

        let mut conn = Connection::new(false, false);
        let modules = if page.is_from_front() {
            module_range.take(page.limit()).collect()
        } else {
            let mut ms: Vec<_> = module_range.rev().take(page.limit()).collect();
            ms.reverse();
            ms
        };

        conn.has_previous_page = modules.first().is_some_and(|fst| {
            parsed
                .modules()
                .range::<String, _>((B::Unbounded, B::Excluded(fst)))
                .next()
                .is_some()
        });

        conn.has_next_page = modules.last().is_some_and(|lst| {
            parsed
                .modules()
                .range::<String, _>((B::Excluded(lst), B::Unbounded))
                .next()
                .is_some()
        });

        for module in modules {
            conn.edges.push(Edge::new(
                JsonCursor::new(module.clone()).encode_cursor(),
                MoveModule::with_fq_name(self.clone(), module),
            ));
        }

        Ok(Some(conn))
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module name, followed by module bytes), in alphabetic order by module name.
    async fn module_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(native.serialized_module_map())
            .context("Failed to serialize module map")?;
        Ok(Some(bytes.into()))
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Option<Vec<Balance>>, RpcError<balance::Error>> {
        self.super_.multi_get_balances(ctx, keys).await
    }

    /// Objects owned by this package, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Fetch the package as an object with the same ID, at a different version, root version bound, or checkpoint.
    ///
    /// If no additional bound is provided, the latest version of this object is fetched at the latest checkpoint.
    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this package, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        self.super_.object_bcs(ctx).await
    }

    /// Paginate all versions of this package treated as an object, after this one.
    pub(crate) async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        self.super_
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
    }

    /// Paginate all versions of this package treated as an object, before this one.
    pub(crate) async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        self.super_
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Result<Option<Owner>, RpcError> {
        self.super_.owner(ctx).await
    }

    /// Fetch the package with the same original ID, at a different version, root version bound, or checkpoint.
    ///
    /// If no additional bound is provided, the latest version of this package is fetched at the latest checkpoint.
    async fn package_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<MovePackage>, RpcError<Error>> {
        MovePackage::by_key(
            ctx,
            self.super_.super_.scope.clone(),
            PackageKey {
                address: self.super_.super_.address.into(),
                version,
                at_checkpoint: checkpoint,
            },
        )
        .await
    }

    /// The Base64-encoded BCS serialization of this package, as a `MovePackage`.
    async fn package_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(native).context("Failed to serialize MovePackage")?;
        Ok(Some(Base64(bytes)))
    }

    /// Paginate all versions of this package after this one.
    async fn package_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, MovePackage>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MovePackage", "packageVersionsAfter");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(version) = self.super_.version(ctx).await? else {
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
            MovePackage::paginate_by_version(
                ctx,
                self.super_.super_.scope.clone(),
                page,
                self.super_.super_.address,
                filter,
            )
            .await?,
        ))
    }

    /// Paginate all versions of this package before this one.
    async fn package_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Option<Connection<String, MovePackage>>, RpcError> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MovePackage", "packageVersionsBefore");
        let page = Page::from_params(limits, first, after, last, before)?;

        let Some(version) = self.super_.version(ctx).await? else {
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
            MovePackage::paginate_by_version(
                ctx,
                self.super_.super_.scope.clone(),
                page,
                self.super_.super_.address,
                filter,
            )
            .await?,
        ))
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError> {
        self.super_.previous_transaction(ctx).await
    }

    /// The transitive dependencies of this package.
    async fn linkage(&self, ctx: &Context<'_>) -> Result<Option<Vec<Linkage>>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let linkage = native
            .linkage_table()
            .iter()
            .map(|(object_id, upgrade_info)| Linkage {
                object_id,
                upgrade_info,
            })
            .collect();

        Ok(Some(linkage))
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<BigInt>, RpcError> {
        self.super_.storage_rebate(ctx).await
    }

    /// The transactions that sent objects to this object.
    pub(crate) async fn received_transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        filter: Option<TransactionFilter>,
    ) -> Result<Option<Connection<String, Transaction>>, RpcError> {
        self.super_
            .received_transactions(ctx, first, after, last, before, filter)
            .await
    }

    /// A table identifying which versions of a package introduced each of its types.
    async fn type_origins(&self, ctx: &Context<'_>) -> Result<Option<Vec<TypeOrigin>>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let type_origins = native
            .type_origin_table()
            .iter()
            .map(|native| TypeOrigin::from(native.clone()))
            .collect();

        Ok(Some(type_origins))
    }
}

impl MovePackage {
    /// Construct a package that is represented by just its address. This does not check that the
    /// object exists, or is a package, so should not be used to "fetch" an address provided as
    /// user input. When the package's contents are fetched from the latest version of that object
    /// as of the current checkpoint.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        // TODO: Look for the package in the scope (just-published packages).
        let super_ = Object::with_address(scope, address);
        Self {
            super_,
            native: Arc::new(OnceCell::new()),
            parsed: Arc::new(OnceCell::new()),
        }
    }

    /// Create a `MovePackage` directly from a `NativeObject`. Returns `None` if the object
    /// is not a package. This is more efficient when you already have the native object.
    pub(crate) fn from_native_object(scope: Scope, native: NativeObject) -> Option<Self> {
        let package = native.data.try_as_package()?.clone();
        let scope = scope.with_root_version(package.version().value());
        let super_ = Object::from_contents(scope, native);
        Some(Self {
            super_,
            native: Arc::new(OnceCell::from(Some(package))),
            parsed: Arc::new(OnceCell::new()),
        })
    }

    /// Try to downcast an `Object` to a `MovePackage`. This function returns `None` if `object`'s
    /// contents cannot be fetched, or it is not a package.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(super_contents) = object.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let Some(package) = super_contents.data.try_as_package().cloned() else {
            return Ok(None);
        };

        Ok(Some(Self {
            super_: object.clone(),
            native: Arc::new(OnceCell::from(Some(package))),
            parsed: Arc::new(OnceCell::new()),
        }))
    }

    /// Fetch a package by its key. The key can either specify an exact version to fetch, an
    /// upperbound against a checkpoint, or neither. Returns `None` when no checkpoint is set
    /// in scope (e.g. execution scope) and no explicit version is provided.
    pub(crate) async fn by_key(
        ctx: &Context<'_>,
        scope: Scope,
        key: PackageKey,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let bounds = key.version.is_some() as u8 + key.at_checkpoint.is_some() as u8;

        if bounds > 1 {
            Err(bad_user_input(Error::OneBound))
        } else if let Some(v) = key.version {
            Self::at_version(ctx, scope, key.address, v)
                .await
                .map_err(upcast)
        } else if let Some(cp) = key.at_checkpoint {
            let scope = scope
                .with_checkpoint_viewed_at(cp.into())
                .ok_or_else(|| bad_user_input(Error::Future(cp.into())))?;

            Self::checkpoint_bounded(ctx, scope, key.address, cp)
                .await
                .map_err(upcast)
        } else if let Some(cp) = scope.checkpoint_viewed_at() {
            Self::checkpoint_bounded(ctx, scope, key.address, cp.into())
                .await
                .map_err(upcast)
        } else {
            Ok(None)
        }
    }

    /// Fetch the package whose original ID matches the original ID of the package at `address`,
    /// but whose version is `version`.
    pub(crate) async fn at_version(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let Some(stored_original) = pg_loader
            .load_one(PackageOriginalIdKey(address.into()))
            .await
            .context("Failed to fetch package original ID")?
        else {
            return Ok(None);
        };

        let original_id = ObjectID::from_bytes(&stored_original.original_id)
            .context("Failed to deserialize ObjectID")?;

        let Some(stored_package) = pg_loader
            .load_one(VersionedOriginalPackageKey(original_id, version.into()))
            .await
            .context("Failed to load package")?
        else {
            return Ok(None);
        };

        let scope = scope.with_root_version(stored_package.package_version as u64);
        Self::from_stored(scope, stored_package)
    }

    /// Fetch the package whose original ID matches the original ID of the package at `address`,
    /// but whose version is latest among all packages that existed `at_checkpoint`.
    pub(crate) async fn checkpoint_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        at_checkpoint: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let Some(stored_original) = pg_loader
            .load_one(PackageOriginalIdKey(address.into()))
            .await
            .context("Failed to fetch package original ID")?
        else {
            return Ok(None);
        };

        let original_id = ObjectID::from_bytes(&stored_original.original_id)
            .context("Failed to deserialize ObjectID")?;

        let Some(stored_package) = pg_loader
            .load_one(CheckpointBoundedOriginalPackageKey(
                original_id,
                at_checkpoint.into(),
            ))
            .await
            .context("Failed to load package")?
        else {
            return Ok(None);
        };

        Self::from_stored(scope, stored_package)
    }

    /// Construct a GraphQL representation of a `MovePackage` from its representation in the
    /// database.
    ///
    /// Returns `None` when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) fn from_stored(
        scope: Scope,
        stored: StoredPackage,
    ) -> Result<Option<Self>, RpcError> {
        if scope
            .checkpoint_viewed_at()
            .is_none_or(|cp| stored.cp_sequence_number as u64 > cp)
        {
            return Ok(None);
        }

        let native: NativeObject = bcs::from_bytes(&stored.serialized_object)
            .context("Failed to deserialize package as object")?;

        let Some(package) = native.data.try_as_package().cloned() else {
            return Ok(None);
        };

        let super_ = Object::from_contents(scope, native);
        Ok(Some(Self {
            super_,
            native: Arc::new(OnceCell::from(Some(package))),
            parsed: Arc::new(OnceCell::new()),
        }))
    }

    /// Paginate through versions of a package, identified by its original ID. `address` points to
    /// any package on-chain that has that original ID.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate_by_version(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CVersion>,
        address: NativeSuiAddress,
        filter: VersionFilter,
    ) -> Result<Connection<String, MovePackage>, RpcError> {
        use kv_packages::dsl as p;

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let pg_reader: &PgReader = ctx.data()?;

        let Some(original_id) = pg_loader
            .load_one(PackageOriginalIdKey(address.into()))
            .await
            .with_context(|| format!("Failed to fetch original package ID for {address}"))?
        else {
            // No original ID record for this package, so it either doesn't exist on chain, or it
            // is not a package.
            return Ok(Connection::new(false, false));
        };

        // The original ID record exists but points to a package that is not visible at the
        // checkpoint being viewed.
        if original_id.cp_sequence_number as u64 > checkpoint_viewed_at {
            return Ok(Connection::new(false, false));
        }

        let mut query = p::kv_packages
            .filter(p::cp_sequence_number.le(checkpoint_viewed_at as i64))
            .filter(p::original_id.eq(original_id.original_id))
            .limit(page.limit() as i64 + 2)
            .into_boxed();

        if let Some(after_version) = filter.after_version {
            query = query.filter(p::package_version.gt(i64::from(after_version)));
        }

        if let Some(before_version) = filter.before_version {
            query = query.filter(p::package_version.lt(i64::from(before_version)));
        }

        query = if page.is_from_front() {
            query
                .order_by(p::cp_sequence_number)
                .then_order_by(p::package_version)
        } else {
            query
                .order_by(p::cp_sequence_number.desc())
                .then_order_by(p::package_version.desc())
        };

        if let Some(after) = page.after() {
            query = query.filter(p::package_version.ge(**after as i64));
        }

        if let Some(before) = page.before() {
            query = query.filter(p::package_version.le(**before as i64));
        }

        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let mut results: Vec<StoredPackage> = c
            .results(query)
            .await
            .context("Failed to read from database")?;

        if !page.is_from_front() {
            results.reverse();
        }

        page.paginate_results(
            results,
            |p| JsonCursor::new(p.package_version as u64),
            |p| {
                let scope = scope.with_root_version(p.package_version as u64);
                Ok(Self::from_stored(scope, p)?.context("Failed to instantiate package")?)
            },
        )
    }

    /// Paginate through all packages published in a range of checkpoints.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate_by_checkpoint(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CPackage>,
        filter: PackageCheckpointFilter,
    ) -> Result<Connection<String, MovePackage>, RpcError> {
        use kv_packages::dsl as p;

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(Connection::new(false, false));
        };

        let pg_reader: &PgReader = ctx.data()?;

        let mut query = p::kv_packages
            .filter(p::cp_sequence_number.le(checkpoint_viewed_at as i64))
            .limit(page.limit() as i64 + 2)
            .into_boxed();

        if let Some(after_cp) = filter.after_checkpoint {
            query = query.filter(p::cp_sequence_number.gt(i64::from(after_cp)));
        }

        if let Some(before_cp) = filter.before_checkpoint {
            query = query.filter(p::cp_sequence_number.lt(i64::from(before_cp)));
        }

        query = if page.is_from_front() {
            query
                .order_by(p::cp_sequence_number)
                .then_order_by(p::original_id)
                .then_order_by(p::package_version)
        } else {
            query
                .order_by(p::cp_sequence_number.desc())
                .then_order_by(p::original_id.desc())
                .then_order_by(p::package_version.desc())
        };

        if let Some(after) = page.after() {
            query = query.filter(sql!(as Bool,
                "(cp_sequence_number, original_id, package_version) >= ({BigInt}, {Bytea}, {BigInt})",
                after.cp_sequence_number as i64,
                after.original_id.as_slice(),
                after.package_version as i64,
            ));
        }

        if let Some(before) = page.before() {
            query = query.filter(sql!(as Bool,
                "(cp_sequence_number, original_id, package_version) <= ({BigInt}, {Bytea}, {BigInt})",
                before.cp_sequence_number as i64,
                before.original_id.as_slice(),
                before.package_version as i64,
            ));
        }

        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let mut results: Vec<StoredPackage> = c
            .results(query)
            .await
            .context("Failed to read from database")?;

        if !page.is_from_front() {
            results.reverse();
        }

        page.paginate_results(
            results,
            |p| {
                BcsCursor::new(PackageCursor {
                    cp_sequence_number: p.cp_sequence_number as u64,
                    original_id: p.original_id.clone(),
                    package_version: p.package_version as u64,
                })
            },
            |p| Ok(Self::from_stored(scope.clone(), p)?.context("Failed to instantiate package")?),
        )
    }

    /// Get the native MovePackage, loading it lazily if needed.
    pub(crate) async fn native(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<NativeMovePackage>, RpcError> {
        self.native
            .get_or_try_init(async || {
                let Some(contents) = self.super_.contents(ctx).await?.as_ref() else {
                    return Ok(None);
                };

                let native = contents
                    .data
                    .try_as_package()
                    .context("Object is not a MovePackage")?;

                Ok(Some(native.clone()))
            })
            .await
    }

    /// Get the parsed representation of this package, loading it lazily if needed.
    pub(crate) async fn parsed(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<ParsedMovePackage>, RpcError> {
        self.parsed
            .get_or_try_init(async || {
                let Some(native) = self.native(ctx).await?.as_ref() else {
                    return Ok(None);
                };

                let parsed = ParsedMovePackage::read_from_package(native)
                    .context("Failed to parse MovePackage")?;

                Ok(Some(parsed))
            })
            .await
    }

    /// Paginate through versions of a package, identified by its original ID. `address` points to
    /// any package on-chain that has that original ID.
    pub(crate) async fn paginate_system_packages(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CSysPackage>,
        checkpoint: u64,
    ) -> Result<Connection<String, MovePackage>, RpcError> {
        let pg_reader: &PgReader = ctx.data()?;

        let mut pagination = query!("");
        if let Some(after) = page.after() {
            pagination += query!(" AND {Bytea} <= original_id", after.as_slice());
        }

        if let Some(before) = page.before() {
            pagination += query!(" AND original_id <= {Bytea}", before.as_slice());
        }

        let query = query!(
            r#"
            SELECT
                v.*
            FROM (
                SELECT DISTINCT
                    original_id
                FROM
                    kv_packages
                WHERE
                    is_system_package
                AND cp_sequence_number <= {BigInt}
                {}
                ORDER BY {}
                LIMIT {BigInt}
            ) k
            CROSS JOIN LATERAL (
                SELECT
                    *
                FROM
                    kv_packages
                WHERE
                    original_id = k.original_id
                AND cp_sequence_number <= {BigInt}
                ORDER BY
                    cp_sequence_number DESC,
                    package_version DESC
                LIMIT
                    1
            ) v
            "#,
            checkpoint as i64,
            pagination,
            if page.is_from_front() {
                query!("original_id")
            } else {
                query!("original_id DESC")
            },
            page.limit() as i64 + 2,
            checkpoint as i64,
        );

        let mut c = pg_reader
            .connect()
            .await
            .context("Failed to connect to database")?;

        let mut results: Vec<StoredPackage> = c
            .results(query)
            .await
            .context("Failed to read from database")?;

        if !page.is_from_front() {
            results.reverse();
        }

        page.paginate_results(
            results,
            |p| BcsCursor::new(p.original_id.clone()),
            |p| Ok(Self::from_stored(scope.clone(), p)?.context("Failed to instantiate package")?),
        )
    }
}
