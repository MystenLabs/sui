// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::InputObject;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::PageInfo;
use async_graphql::dataloader::DataLoader;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::sql_types::Bool;
use prost_types::FieldMask;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::objects::VersionedObjectKey;
use sui_indexer_alt_reader::packages::CheckpointBoundedOriginalPackageKey;
use sui_indexer_alt_reader::packages::PackageOriginalIdKey;
use sui_indexer_alt_reader::packages::VersionedOriginalPackageKey;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_schema::packages::StoredPackage;
use sui_indexer_alt_schema::schema::kv_packages;
use sui_package_resolver::Package as ParsedMovePackage;
use sui_pg_db::sql;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_sql_macro::query;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::effects::TransactionEffects as NativeTransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::move_package::MovePackage as NativeMovePackage;
use sui_types::object::Object as NativeObject;
use tokio::sync::OnceCell;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::big_int::BigInt;
use crate::api::scalars::cursor::BcsCursor;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::cursor::MultiCursor;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::digest::Digest;
use crate::api::scalars::id::Id;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeInput;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address;
use crate::api::types::address::Address;
use crate::api::types::balance;
use crate::api::types::balance::Balance;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::linkage::Linkage;
use crate::api::types::move_module::MoveModule;
use crate::api::types::move_object::MoveObject;
use crate::api::types::name_record::NameRecord;
use crate::api::types::object;
use crate::api::types::object::CLive;
use crate::api::types::object::CVersion;
use crate::api::types::object::Object;
use crate::api::types::object::VersionFilter;
use crate::api::types::object_filter::ObjectFilter;
use crate::api::types::object_filter::ObjectFilterValidator as OFValidator;
use crate::api::types::owner::Owner;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionConnection;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction_object::TransactionObject;
use crate::api::types::type_origin::TypeOrigin;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::extensions::query_limits;
use crate::pagination;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::pagination::StreamConnection;
use crate::scope::Scope;
use crate::task::watermark::Watermarks;

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
/// The `address` field must be specified, as well as at most one of `version`, or `atCheckpoint`. If neither is provided, the package is fetched at the checkpoint being viewed.
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

/// Inner struct for the cursor produced while iterating over all package publishes on the
/// Postgres path, ordered by `(cp_sequence_number, original_id, package_version)`.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub(crate) struct PackageCursor {
    pub cp_sequence_number: u64,
    pub original_id: Vec<u8>,
    pub package_version: u64,
}

/// Cursor for iterating over package publishes on the gRPC path, which scans transactions that
/// wrote packages, in transaction order.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Copy)]
pub struct PackageToken {
    /// Hint for the checkpoint the transaction belongs to (0 = unknown).
    checkpoint: u64,
    position: PackagePosition,
}

/// Where a [`PackageToken`] sits in the scan. Enriches the wire's item vs. watermark cursor
/// distinction: a cursor either points at a served package write, or at scan progress through a
/// transaction that served nothing.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Copy)]
enum PackagePosition {
    /// Points at one package write within a transaction, by its position in effects order. The
    /// transaction may hold writes on either side of this one, so resuming from here re-fetches
    /// the transaction and skips the writes on the cursor's served side (at-or-before it going
    /// forwards, at-or-after it going backwards).
    Tx { tx_seq: u64, write_index: u32 },

    /// A scan frontier: the wire scanned through this transaction but served nothing from it.
    /// Resume strictly past it, in either direction.
    Scan { tx_seq: u64 },
}

/// The pure pagination of a scanned page: the package writes it serves and the page-info values
/// describing how to resume around it.
#[derive(Debug)]
struct PackagePage {
    /// The served package writes, in scan order: resume token, storage ID, and version.
    writes: Vec<(PackageToken, ObjectID, u64)>,
    has_previous_page: bool,
    has_next_page: bool,
    start: Option<PackageToken>,
    end: Option<PackageToken>,
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

/// Cursor for iterating over package publishes. gRPC iterates on transaction order, while Postgres
/// is on `(checkpoint, original_id, version)`, so the cursors are not interchangeable.
pub(crate) type CPackage = MultiCursor<OpaqueCursor<PackageToken>, BcsCursor<PackageCursor>>;

/// Custom `Connection` for packages, to support the partially-filled pages the gRPC scan can
/// produce.
pub(crate) type MovePackageConnection = StreamConnection<MovePackage>;

/// Cursor for iterating over system packages. Points at a particular system package, by its ID.
pub(crate) type CSysPackage = BcsCursor<Vec<u8>>;

/// A MovePackage is a kind of Object that represents code that has been published on-chain. It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
    /// The package's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        Id::MovePackage(self.super_.super_.address)
    }

    /// The MovePackage's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// Fetch the address as it was at a different root version, or checkpoint.
    ///
    /// If no additional bound is provided, the address is fetched at the latest checkpoint known to the RPC.
    pub(crate) async fn address_at(
        &self,
        ctx: &Context<'_>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Option<Result<Address, RpcError<address::Error>>> {
        self.super_
            .address_at(ctx, root_version, checkpoint)
            .await
            .ok()?
    }

    /// The version of this package that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Option<Result<UInt53, RpcError>> {
        self.super_.version(ctx).await.ok()?
    }

    /// 32-byte hash that identifies the package's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Option<Result<String, RpcError>> {
        self.super_.digest(ctx).await.ok()?
    }

    /// How this object was referenced by a specific transaction.
    ///
    /// Returns `null` if the object was not referenced, or was present only as a non-object marker variant of unchanged consensus input (e.g. cancelled, stream-ended, per-epoch).
    ///
    /// The `transactionDigest` argument may be omitted when the query is scoped under a transaction context (e.g. a parent `Transaction`, `TransactionEffects`, or `Event`); the field then resolves against the in-scope transaction.
    ///
    /// Passing an explicit `transactionDigest` other than the in-scope transaction in subscription context is not supported; for arbitrary transaction lookups, use the indexed Query API.
    pub(crate) async fn as_transaction_object(
        &self,
        ctx: &Context<'_>,
        transaction_digest: Option<Digest>,
    ) -> Option<Result<TransactionObject, RpcError>> {
        self.super_
            .as_transaction_object(ctx, transaction_digest)
            .await
            .ok()?
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Option<Result<Balance, RpcError<balance::Error>>> {
        self.super_.balance(ctx, coin_type).await.ok()?
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Option<Result<Connection<String, Balance>, RpcError<balance::Error>>> {
        self.super_
            .balances(ctx, first, after, last, before)
            .await
            .ok()?
    }

    /// The domain explicitly configured as the default Name Service name for this address.
    pub(crate) async fn default_name_record(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<NameRecord, RpcError<object::Error>>> {
        self.super_.default_name_record(ctx).await.ok()?
    }

    /// The module named `name` in this package.
    async fn module(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Option<Result<MoveModule, RpcError>> {
        let parsed = self.parsed(ctx).await.ok()?.as_ref()?;

        if parsed.module(&name).is_err() {
            return None;
        }

        Some(Ok(MoveModule::with_fq_name(self.clone(), name)))
    }

    /// Paginate through this package's modules.
    async fn modules(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CModule>,
        last: Option<u64>,
        before: Option<CModule>,
    ) -> Option<Result<Connection<String, MoveModule>, RpcError>> {
        use std::ops::Bound as B;

        async {
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
        .await
        .transpose()
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module name, followed by module bytes), in alphabetic order by module name.
    async fn module_bcs(&self, ctx: &Context<'_>) -> Option<Result<Base64, RpcError>> {
        async {
            let Some(native) = self.native(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let bytes = bcs::to_bytes(native.serialized_module_map())
                .context("Failed to serialize module map")?;
            Ok(Some(bytes.into()))
        }
        .await
        .transpose()
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Option<Result<Vec<Balance>, RpcError<balance::Error>>> {
        self.super_.multi_get_balances(ctx, keys).await.ok()?
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
    ) -> Option<Result<Connection<String, MoveObject>, RpcError<object::Error>>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
            .ok()?
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
    ) -> Option<Result<Object, RpcError<object::Error>>> {
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
            .ok()?
    }

    /// The Base64-encoded BCS serialization of this package, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Option<Result<Base64, RpcError>> {
        self.super_.object_bcs(ctx).await.ok()?
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
    ) -> Option<Result<Connection<String, Object>, RpcError>> {
        self.super_
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
            .ok()?
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
    ) -> Option<Result<Connection<String, Object>, RpcError>> {
        self.super_
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Option<Result<Owner, RpcError>> {
        self.super_.owner(ctx).await.ok()?
    }

    /// Fetch the package with the same original ID, at a different version, or checkpoint.
    ///
    /// If no additional bound is provided, the package is fetched at the latest checkpoint known to the RPC.
    async fn package_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Option<Result<MovePackage, RpcError<Error>>> {
        async {
            let key = PackageKey {
                address: self.super_.super_.address.into(),
                version,
                at_checkpoint: checkpoint,
            };

            MovePackage::by_key(ctx, Scope::new(ctx)?, key).await
        }
        .await
        .transpose()
    }

    /// The Base64-encoded BCS serialization of this package, as a `MovePackage`.
    async fn package_bcs(&self, ctx: &Context<'_>) -> Option<Result<Base64, RpcError>> {
        async {
            let Some(native) = self.native(ctx).await?.as_ref() else {
                return Ok(None);
            };

            let bytes = bcs::to_bytes(native).context("Failed to serialize MovePackage")?;
            Ok(Some(Base64(bytes)))
        }
        .await
        .transpose()
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
    ) -> Option<Result<MovePackageConnection, RpcError>> {
        let version = self.version(ctx).await.ok()??;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("MovePackage", "packageVersionsAfter");
                let page = Page::from_params(limits, first, after, last, before)?;
                let version = version?;

                // Apply any filter that was supplied to the query, but add an additional version
                // lowerbound constraint.
                let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
                    after_version: Some(version),
                    ..VersionFilter::default()
                }) else {
                    return Ok(MovePackageConnection::empty());
                };

                MovePackage::paginate_by_version(
                    ctx,
                    self.super_.super_.scope.clone(),
                    page,
                    self.super_.super_.address,
                    filter,
                )
                .await
                .map(Into::into)
            }
            .await,
        )
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
    ) -> Option<Result<MovePackageConnection, RpcError>> {
        let version = self.version(ctx).await.ok()??;

        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("MovePackage", "packageVersionsBefore");
                let page = Page::from_params(limits, first, after, last, before)?;
                let version = version?;

                // Apply any filter that was supplied to the query, but add an additional version
                // upperbound constraint.
                let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
                    before_version: Some(version),
                    ..VersionFilter::default()
                }) else {
                    return Ok(MovePackageConnection::empty());
                };

                MovePackage::paginate_by_version(
                    ctx,
                    self.super_.super_.scope.clone(),
                    page,
                    self.super_.super_.address,
                    filter,
                )
                .await
                .map(Into::into)
            }
            .await,
        )
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Transaction, RpcError>> {
        self.super_.previous_transaction(ctx).await.ok()?
    }

    /// The transitive dependencies of this package.
    async fn linkage(&self, ctx: &Context<'_>) -> Option<Result<Vec<Linkage<'_>>, RpcError>> {
        let native = self.native(ctx).await.ok()?.as_ref()?;

        let linkage = native
            .linkage_table()
            .iter()
            .map(|(object_id, upgrade_info)| Linkage {
                object_id,
                upgrade_info,
            })
            .collect();

        Some(Ok(linkage))
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<BigInt, RpcError>> {
        self.super_.storage_rebate(ctx).await.ok()?
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
    ) -> Option<Result<TransactionConnection, RpcError>> {
        self.super_
            .received_transactions(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// A table identifying which versions of a package introduced each of its types.
    async fn type_origins(&self, ctx: &Context<'_>) -> Option<Result<Vec<TypeOrigin>, RpcError>> {
        let native = self.native(ctx).await.ok()?.as_ref()?;

        let type_origins = native
            .type_origin_table()
            .iter()
            .map(|native| TypeOrigin::from(native.clone()))
            .collect();

        Some(Ok(type_origins))
    }
}

impl MovePackage {
    /// Construct a package that is represented by just its address. This does not check that the
    /// object exists, or is a package, so should not be used to "fetch" an address provided as
    /// user input. When the package's contents are fetched from the latest version of that object
    /// as of the current checkpoint.
    pub(crate) fn with_address(scope: Scope, address: NativeSuiAddress) -> Self {
        let super_ = Object::with_address(scope, address);
        Self {
            super_,
            native: Arc::new(OnceCell::new()),
            parsed: Arc::new(OnceCell::new()),
        }
    }

    /// Try to downcast an `Object` to a `MovePackage`. This function returns `None` if `object`'s
    /// contents cannot be fetched, or it is not a package.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(super_contents) = object.contents(ctx).await? else {
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
            // Validate checkpoint isn't in the future
            let watermark: &Arc<Watermarks> = ctx.data()?;
            if u64::from(cp) > watermark.high_watermark().checkpoint() {
                return Err(bad_user_input(Error::Future(cp.into())));
            }

            // checkpoint_bounded sets the root checkpoint bound
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

        Self::from_stored(
            scope.with_root_checkpoint(at_checkpoint.into()),
            stored_package,
        )
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

        Ok(Self::from_object_contents(scope, native))
    }

    /// Construct a GraphQL representation of a `MovePackage` from a full native object. Returns
    /// `None` if the object is not a package.
    pub(crate) fn from_object_contents(scope: Scope, native: NativeObject) -> Option<Self> {
        let package = native.data.try_as_package().cloned()?;

        let super_ = Object::from_contents(scope, native);
        Some(Self {
            super_,
            native: Arc::new(OnceCell::from(Some(package))),
            parsed: Arc::new(OnceCell::new()),
        })
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

        query_limits::rich::debit(ctx)?;
        let pg_reader: &PgReader = ctx.data()?;
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

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
    ) -> Result<MovePackageConnection, RpcError> {
        use kv_packages::dsl as p;

        query_limits::rich::debit(ctx)?;

        if let Some(reader) = ctx.data_opt::<AlphaLedgerGrpcReader>() {
            return Self::paginate_grpc_by_checkpoint(ctx, reader, scope, page, filter).await;
        }

        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(MovePackageConnection::empty());
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
            let after = after.legacy()?;
            query = query.filter(sql!(as Bool,
                "(cp_sequence_number, original_id, package_version) >= ({BigInt}, {Bytea}, {BigInt})",
                after.cp_sequence_number as i64,
                after.original_id.as_slice(),
                after.package_version as i64,
            ));
        }

        if let Some(before) = page.before() {
            let before = before.legacy()?;
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
                CPackage::Secondary(BcsCursor::new(PackageCursor {
                    cp_sequence_number: p.cp_sequence_number as u64,
                    original_id: p.original_id.clone(),
                    package_version: p.package_version as u64,
                }))
            },
            |p| Ok(Self::from_stored(scope.clone(), p)?.context("Failed to instantiate package")?),
        )
        .map(Into::into)
    }

    /// Serve package pagination from ledger history: scan transactions that wrote packages (the
    /// `AnyPackageWrite` bitmap, via the `package_write` filter predicate), expand each into its
    /// package writes from the effects, and load the package contents by the exact `(id, version)`
    /// the effects reference. Iterates in transaction order, unlike the Postgres path's
    /// `(checkpoint, original_id, version)` order. Pages may be partially filled, with valid
    /// cursors if there are more pages to paginate through.
    async fn paginate_grpc_by_checkpoint(
        ctx: &Context<'_>,
        reader: &AlphaLedgerGrpcReader,
        scope: Scope,
        page: Page<CPackage>,
        filter: PackageCheckpointFilter,
    ) -> Result<MovePackageConnection, RpcError> {
        if page.limit() == 0 {
            return Ok(MovePackageConnection::empty());
        }

        // Consistency upper bound; empty when scope has no checkpoint set.
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(MovePackageConnection::empty());
        };

        // TODO: LedgerService expose available checkpoint range for `reader_lo`.
        let reader_lo = 0;

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            None,
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(MovePackageConnection::empty());
        };

        let after = page.after().map(|c| c.token()).transpose()?;
        let before = page.before().map(|c| c.token()).transpose()?;

        let request = build_scan_request(&page, &cp_bounds, after.as_ref(), before.as_ref());

        let result = reader
            .list_transactions(request)
            .await
            .context("Failed to list transactions")?;

        Self::build_grpc_package_connection(ctx, scope, &page, after, before, result).await
    }

    /// Translate a page of package-writing transactions into a page of packages: work out the
    /// served writes and resume cursors with [`paginate_package_writes`], then load package
    /// contents in one batched lookup.
    async fn build_grpc_package_connection(
        ctx: &Context<'_>,
        scope: Scope,
        page: &Page<CPackage>,
        after: Option<PackageToken>,
        before: Option<PackageToken>,
        result: StreamPage<v2::ExecutedTransaction>,
    ) -> Result<MovePackageConnection, RpcError> {
        let PackagePage {
            writes,
            has_previous_page,
            has_next_page,
            start,
            end,
        } = paginate_package_writes(page, after, before, &result)?;

        let kv_loader: &KvLoader = ctx.data()?;
        let objects = kv_loader
            .load_many_objects(
                writes
                    .iter()
                    .map(|(_, id, version)| VersionedObjectKey(*id, *version))
                    .collect(),
            )
            .await
            .context("Failed to load package objects")?;

        let mut edges = Vec::with_capacity(writes.len());
        for (token, id, version) in &writes {
            let object = objects
                .get(&VersionedObjectKey(*id, *version))
                .with_context(|| format!("Missing package object {id} at version {version}"))?;

            let package = Self::from_object_contents(scope.clone(), object.clone())
                .with_context(|| format!("Object {id} written as a package is not a package"))?;

            let cursor = CPackage::new(OpaqueCursor::new(*token)).encode_cursor();
            edges.push(Edge::new(cursor, package));
        }

        // Writes are in scan order; a backward page presents them ascending.
        if !page.is_from_front() {
            edges.reverse();
        }

        let encode = |t: PackageToken| CPackage::new(OpaqueCursor::new(t)).encode_cursor();
        Ok(MovePackageConnection {
            edges,
            page_info: PageInfo {
                has_previous_page,
                has_next_page,
                start_cursor: start.map(encode),
                end_cursor: end.map(encode),
            },
        })
    }

    /// Paginate through versions of a package, identified by its original ID. `address` points to
    /// any package on-chain that has that original ID.
    pub(crate) async fn paginate_system_packages(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CSysPackage>,
        checkpoint: u64,
    ) -> Result<Connection<String, MovePackage>, RpcError> {
        query_limits::rich::debit(ctx)?;
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

    /// The package's address
    pub(crate) fn address_impl(&self) -> NativeSuiAddress {
        self.super_.super_.address
    }

    /// Get the native MovePackage, loading it lazily if needed.
    pub(crate) async fn native(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<NativeMovePackage>, RpcError> {
        self.native
            .get_or_try_init(async || {
                let Some(contents) = self.super_.contents(ctx).await? else {
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
                let Some(native) = self.native(ctx).await? else {
                    return Ok(None);
                };

                let parsed = ParsedMovePackage::read_from_package(native)
                    .context("Failed to parse MovePackage")?;

                Ok(Some(parsed))
            })
            .await
    }
}

impl PackageToken {
    /// Converts an encoded `CursorToken` (a transaction position) into a scan frontier.
    fn from_cursor(bytes: &[u8]) -> Result<Self, RpcError> {
        let token = CursorToken::decode(bytes).context("Failed to decode stream cursor")?;
        let Position::Transactions { checkpoint, tx_seq } = token.position else {
            return Err(anyhow::anyhow!("Unexpected position in stream cursor").into());
        };
        Ok(PackageToken {
            checkpoint,
            position: PackagePosition::Scan { tx_seq },
        })
    }

    /// The position of one package write within this token's transaction.
    fn at(self, write_index: u32) -> Self {
        Self {
            checkpoint: self.checkpoint,
            position: PackagePosition::Tx {
                tx_seq: self.tx_seq(),
                write_index,
            },
        }
    }

    fn tx_seq(&self) -> u64 {
        match self.position {
            PackagePosition::Tx { tx_seq, .. } | PackagePosition::Scan { tx_seq } => tx_seq,
        }
    }

    /// Whether the write at `write_index` of transaction `tx_seq` is behind this token when it is
    /// used as an `after` bound.
    fn covers_up_to(&self, tx_seq: u64, write_index: u32) -> bool {
        self.tx_seq() == tx_seq
            && match self.position {
                PackagePosition::Tx {
                    write_index: served,
                    ..
                } => write_index <= served,
                PackagePosition::Scan { .. } => true,
            }
    }

    /// Whether the write at `write_index` of transaction `tx_seq` is behind this token when it is
    /// used as a `before` bound.
    fn covers_from(&self, tx_seq: u64, write_index: u32) -> bool {
        self.tx_seq() == tx_seq
            && match self.position {
                PackagePosition::Tx {
                    write_index: served,
                    ..
                } => write_index >= served,
                PackagePosition::Scan { .. } => true,
            }
    }
}

impl CPackage {
    /// View the cursor as a package-write position for the gRPC path. Fails if it was minted by
    /// the Postgres path, whose `(checkpoint, original_id, version)` positions cannot seek a
    /// transaction-order scan.
    fn token(&self) -> Result<PackageToken, RpcError> {
        match self {
            CPackage::Primary(c) => Ok(**c),
            CPackage::Secondary(_) => Err(pagination::Error::UnusableCursor.into()),
        }
    }

    /// View the cursor as a `(checkpoint, original_id, version)` position for the Postgres path.
    /// Fails if it was minted by the gRPC path, whose transaction-order positions cannot seek the
    /// `kv_packages` table.
    fn legacy(&self) -> Result<&PackageCursor, RpcError> {
        match self {
            CPackage::Primary(_) => Err(pagination::Error::UnusableCursor.into()),
            CPackage::Secondary(c) => Ok(&**c),
        }
    }
}

impl Eq for CPackage {}

/// The two wire formats occupy disjoint position spaces, so cursors only compare equal within a
/// format.
impl PartialEq for CPackage {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (CPackage::Primary(a), CPackage::Primary(b)) => a == b,
            (CPackage::Secondary(a), CPackage::Secondary(b)) => a == b,
            _ => false,
        }
    }
}

impl ByteCursor for PackageToken {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(bcs::from_bytes(bytes)?)
    }

    fn encode_cursor(&self) -> bytes::Bytes {
        bcs::to_bytes(self)
            .expect("serialization cannot fail for a plain struct")
            .into()
    }
}

/// Build the `ListTransactions` request scanning `cp_bounds` for package-writing transactions.
///
/// The wire cursors address transactions, so a `Tx` cursor widens to re-include its transaction —
/// the writes it has already covered are skipped client-side during expansion — while a `Scan`
/// cursor bounds on its transaction directly. Checkpoint hints on widened bounds are reset to
/// unknown (0 for `after`, `u64::MAX` for `before`) because the neighboring transaction may fall
/// in a different checkpoint.
fn build_scan_request(
    page: &Page<CPackage>,
    cp_bounds: &RangeInclusive<u64>,
    after: Option<&PackageToken>,
    before: Option<&PackageToken>,
) -> v2::ListTransactionsRequest {
    let wire_after = after.and_then(|t| {
        let position = match t.position {
            // Nothing was served from a scan frontier; bound on it directly.
            PackagePosition::Scan { tx_seq } => Position::Transactions {
                checkpoint: t.checkpoint,
                tx_seq,
            },
            // Nothing precedes the first transaction; resume from the range start and skip
            // client-side.
            PackagePosition::Tx { tx_seq: 0, .. } => return None,
            // The cursor's transaction may hold further writes; re-include it.
            PackagePosition::Tx { tx_seq, .. } => Position::Transactions {
                checkpoint: 0,
                tx_seq: tx_seq - 1,
            },
        };
        Some(CursorToken::item(position).encode())
    });

    let wire_before = before.map(|t| {
        let position = match t.position {
            // A scan frontier, or a cursor at a transaction's first write: everything from the
            // transaction's start onward is on the served side, so bound on it directly.
            PackagePosition::Scan { tx_seq }
            | PackagePosition::Tx {
                tx_seq,
                write_index: 0,
            } => Position::Transactions {
                // A 0 hint would collapse the checkpoint window, so treat it as unknown.
                checkpoint: if t.checkpoint == 0 {
                    u64::MAX
                } else {
                    t.checkpoint
                },
                tx_seq,
            },
            // Writes before the cursor still belong to the backward page; re-include the
            // transaction.
            PackagePosition::Tx { tx_seq, .. } => Position::Transactions {
                checkpoint: u64::MAX,
                tx_seq: tx_seq.saturating_add(1),
            },
        };
        CursorToken::item(position).encode()
    });

    let mut options = v2::QueryOptions::default();
    // One extra transaction: a re-included boundary transaction may contribute no further
    // packages, while every other matched transaction contributes at least one.
    options.limit = Some(page.limit() as u32 + 1);
    options.after = wire_after;
    options.before = wire_before;
    options.ordering = Some(if page.is_from_front() {
        v2::Ordering::Ascending as i32
    } else {
        v2::Ordering::Descending as i32
    });

    let mut request = v2::ListTransactionsRequest::default();
    // Only the effects are needed to identify package writes; contents load separately.
    request.read_mask = Some(FieldMask::from_paths(["effects.bcs"]));
    request.start_checkpoint = Some(*cp_bounds.start());
    // `cp_bounds` end is inclusive; the request bound is exclusive.
    request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
    request.filter = Some(package_write_filter());
    request.options = Some(options);
    request
}

/// Expand a page of package-writing transactions into the page of package writes it serves: skip
/// writes a boundary cursor has already covered, trim to the page size, and work out the resume
/// cursors on both sides.
fn paginate_package_writes(
    page: &Page<CPackage>,
    after: Option<PackageToken>,
    before: Option<PackageToken>,
    result: &StreamPage<v2::ExecutedTransaction>,
) -> Result<PackagePage, RpcError> {
    let more = result.has_more();

    let mut expanded: Vec<(PackageToken, ObjectID, u64)> = vec![];
    for item in &result.items {
        let position = PackageToken::from_cursor(&item.cursor)?;

        let mut writes = package_writes(&item.payload)?;
        if !page.is_from_front() {
            writes.reverse();
        }

        for (write_index, id, version) in writes {
            // Skip writes a boundary cursor has already covered — its transaction was
            // deliberately re-included by the widened wire bounds.
            if after.is_some_and(|a| a.covers_up_to(position.tx_seq(), write_index)) {
                continue;
            }
            if before.is_some_and(|b| b.covers_from(position.tx_seq(), write_index)) {
                continue;
            }

            expanded.push((position.at(write_index), id, version));
        }
    }

    // More matches than the page can hold: trim the scan-direction tail and resume from the last
    // edge kept rather than the scan frontier.
    let trimmed = expanded.len() > page.limit();
    expanded.truncate(page.limit());

    let first_pos = result
        .first_cursor()
        .map(|c| PackageToken::from_cursor(c))
        .transpose()?;
    let last_pos = result
        .last_cursor()
        .map(|c| PackageToken::from_cursor(c))
        .transpose()?;

    let first_edge = expanded.first().map(|(t, _, _)| *t);
    let last_edge = expanded.last().map(|(t, _, _)| *t);

    // The side where the scan entered, and the side where it stopped. A fence naming the same
    // transaction as its boundary edge is that item's own cursor — the transaction served
    // writes, so resume at the edge (`Tx`); otherwise nothing was served from the fence's
    // transaction and it can be scanned past (`Scan`). A trimmed page's far side always resumes
    // from the last kept edge.
    let same_tx = |edge: Option<PackageToken>, fence: Option<PackageToken>| {
        edge.zip(fence)
            .is_some_and(|(edge, fence)| edge.tx_seq() == fence.tx_seq())
    };

    let entry = if same_tx(first_edge, first_pos) {
        first_edge
    } else {
        first_pos
    };

    let far = if trimmed || same_tx(last_edge, last_pos) {
        last_edge
    } else {
        last_pos
    };

    let (has_previous_page, has_next_page, start, end) = if page.is_from_front() {
        (page.after().is_some(), trimmed || more, entry, far)
    } else {
        // Descending: the scan enters at the ascending end of the page, and its far side is the
        // ascending start.
        (trimmed || more, page.before().is_some(), far, entry)
    };

    Ok(PackagePage {
        writes: expanded,
        has_previous_page,
        has_next_page,
        start,
        end,
    })
}

/// Extract a transaction's package writes from its effects: the written refs of published
/// packages, paired with their index in effects order (`written()` preserves it), which
/// `PackageToken::write_index` is defined over.
fn package_writes(
    transaction: &v2::ExecutedTransaction,
) -> Result<Vec<(u32, ObjectID, u64)>, RpcError> {
    let effects: NativeTransactionEffects = transaction
        .effects
        .as_ref()
        .and_then(|fx| fx.bcs.as_ref())
        .context("ListTransactions item missing effects")?
        .deserialize()
        .context("Failed to deserialize effects")?;

    let published: BTreeSet<ObjectID> = effects.published_packages().into_iter().collect();
    Ok(effects
        .written()
        .into_iter()
        .filter(|(id, _, _)| published.contains(id))
        .enumerate()
        .map(|(i, (id, version, _))| (i as u32, id, version.value()))
        .collect())
}

/// A filter matching every transaction that wrote a Move package — first publishes and upgrades
/// alike (the `AnyPackageWrite` bitmap dimension).
fn package_write_filter() -> v2::TransactionFilter {
    let mut literal = v2::TransactionLiteral::default();
    literal.predicate = Some(v2::transaction_literal::Predicate::PackageWrite(
        v2::PackageWriteFilter::default(),
    ));

    v2::TransactionFilter::default().with_terms(vec![
        v2::TransactionTerm::default().with_literals(vec![literal]),
    ])
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;
    use sui_types::base_types::SequenceNumber;
    use sui_types::base_types::random_object_ref;
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::SenderSignedData;
    use sui_types::transaction::TransactionData;

    use crate::pagination::PageLimits;

    use super::*;

    /// The checkpoint hint carried by item cursors in these tests.
    const CP: u64 = 7;

    fn pkg(n: u8) -> ObjectID {
        ObjectID::from_single_byte(n)
    }

    /// Wire cursor bytes for transaction `tx_seq` in `checkpoint`.
    fn wire_cursor(checkpoint: u64, tx_seq: u64) -> Bytes {
        CursorToken::item(Position::Transactions { checkpoint, tx_seq }).encode()
    }

    /// A `PageItem` whose effects publish `packages` as `(id, version)` pairs, with its cursor at
    /// `(CP, tx_seq)`.
    fn pkg_item(tx_seq: u64, packages: &[(ObjectID, u64)]) -> PageItem<v2::ExecutedTransaction> {
        let pt = ProgrammableTransactionBuilder::new().finish();
        let data = TransactionData::new_programmable(
            NativeSuiAddress::ZERO,
            vec![random_object_ref()],
            pt,
            1,
            1,
        );
        let signed = SenderSignedData::new(data, vec![]);

        let effects = TestEffectsBuilder::new(&signed)
            .with_package_writes(
                packages
                    .iter()
                    .map(|(id, version)| (*id, SequenceNumber::from(*version))),
            )
            .build();

        let mut fx = v2::TransactionEffects::default();
        fx.bcs = Some(v2::Bcs::serialize(&effects).expect("serialize effects"));

        let mut payload = v2::ExecutedTransaction::default();
        payload.effects = Some(fx);

        PageItem {
            payload,
            cursor: wire_cursor(CP, tx_seq),
        }
    }

    /// A `Tx` token in checkpoint `CP`.
    fn tx(tx_seq: u64, write_index: u32) -> PackageToken {
        PackageToken {
            checkpoint: CP,
            position: PackagePosition::Tx {
                tx_seq,
                write_index,
            },
        }
    }

    fn scan(checkpoint: u64, tx_seq: u64) -> PackageToken {
        PackageToken {
            checkpoint,
            position: PackagePosition::Scan { tx_seq },
        }
    }

    fn limits(limit: u64) -> PageLimits {
        PageLimits {
            default: limit as u32,
            max: limit as u32,
        }
    }

    fn forward(limit: u64) -> Page<CPackage> {
        Page::from_params(&limits(limit), Some(limit), None, None, None).expect("forward page")
    }

    fn forward_after(limit: u64, after: PackageToken) -> Page<CPackage> {
        let after = CPackage::new(OpaqueCursor::new(after));
        Page::from_params(&limits(limit), Some(limit), Some(after), None, None)
            .expect("forward page with after")
    }

    fn backward(limit: u64) -> Page<CPackage> {
        Page::from_params(&limits(limit), None, None, Some(limit), None).expect("backward page")
    }

    fn backward_before(limit: u64, before: PackageToken) -> Page<CPackage> {
        let before = CPackage::new(OpaqueCursor::new(before));
        Page::from_params(&limits(limit), None, None, Some(limit), Some(before))
            .expect("backward page with before")
    }

    /// The served writes, flattened to `(tx_seq, write_index, id, version)`.
    fn served(page: &PackagePage) -> Vec<(u64, u32, ObjectID, u64)> {
        page.writes
            .iter()
            .map(|(t, id, version)| match t.position {
                PackagePosition::Tx {
                    tx_seq,
                    write_index,
                } => (tx_seq, write_index, *id, *version),
                PackagePosition::Scan { .. } => panic!("edge token must be `Tx`"),
            })
            .collect()
    }

    /// Decode a wire bound back into `(checkpoint, tx_seq)`.
    fn wire_position(bytes: &Bytes) -> (u64, u64) {
        let token = CursorToken::decode(bytes).expect("decode wire cursor");
        let Position::Transactions { checkpoint, tx_seq } = token.position else {
            panic!("expected transactions position, got {:?}", token.position);
        };
        (checkpoint, tx_seq)
    }

    /// Fences that are the boundary items' own cursors resume as `Tx` positions at the boundary
    /// edges.
    #[test]
    fn forward_fences_on_items_mint_tx() {
        let (a, b) = (pkg(1), pkg(2));
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1)]), pkg_item(11, &[(b, 1)])],
            None,
            None,
            Some(v2::QueryEndReason::LedgerTip),
        );

        let page = paginate_package_writes(&forward(5), None, None, &result).expect("paginate");

        assert_eq!(served(&page), vec![(10, 0, a, 1), (11, 0, b, 1)]);
        assert_eq!(page.start, Some(tx(10, 0)));
        assert_eq!(page.end, Some(tx(11, 0)));
        assert!(!page.has_previous_page);
        assert!(!page.has_next_page);
    }

    /// Standalone watermark cursors decode as `Scan` fences: their transactions served nothing,
    /// so resumption seeks strictly past them.
    #[test]
    fn forward_watermark_fences_mint_scan() {
        let a = pkg(1);
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1)])],
            Some(wire_cursor(6, 8)),
            Some(wire_cursor(9, 15)),
            None,
        );

        let page = paginate_package_writes(&forward(5), None, None, &result).expect("paginate");

        assert_eq!(served(&page), vec![(10, 0, a, 1)]);
        assert_eq!(page.start, Some(scan(6, 8)));
        assert_eq!(page.end, Some(scan(9, 15)));
        assert!(page.has_next_page);
    }

    /// A trimmed page resumes from its last kept edge, not the scan frontier.
    #[test]
    fn forward_trim_resumes_from_last_kept_edge() {
        let (a, b, c, d) = (pkg(1), pkg(2), pkg(3), pkg(4));
        let result = StreamPage::for_test(
            vec![
                pkg_item(10, &[(a, 1), (b, 1)]),
                pkg_item(11, &[(c, 1), (d, 1)]),
            ],
            None,
            None,
            Some(v2::QueryEndReason::LedgerTip),
        );

        let page = paginate_package_writes(&forward(3), None, None, &result).expect("paginate");

        assert_eq!(
            served(&page),
            vec![(10, 0, a, 1), (10, 1, b, 1), (11, 0, c, 1)]
        );
        assert_eq!(page.end, Some(tx(11, 0)));
        assert!(page.has_next_page);
    }

    /// An `after` cursor's transaction is re-included by the widened wire bounds; the writes the
    /// cursor already covered are skipped.
    #[test]
    fn forward_after_tx_skips_covered_writes() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1), (b, 1)]), pkg_item(11, &[(c, 1)])],
            None,
            None,
            Some(v2::QueryEndReason::LedgerTip),
        );

        let page = paginate_package_writes(
            &forward_after(5, tx(10, 0)),
            Some(tx(10, 0)),
            None,
            &result,
        )
        .expect("paginate");

        assert_eq!(served(&page), vec![(10, 1, b, 1), (11, 0, c, 1)]);
        assert!(page.has_previous_page);
    }

    /// Backward pages scan descending: writes within each transaction reverse, and the page's
    /// `end` is the scan's entry side while `start` is its far side.
    #[test]
    fn backward_fences_swap_sides() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        // Descending scan order: tx 11 first, then tx 10.
        let result = StreamPage::for_test(
            vec![pkg_item(11, &[(b, 1), (c, 1)]), pkg_item(10, &[(a, 1)])],
            None,
            None,
            Some(v2::QueryEndReason::LedgerTip),
        );

        let page = paginate_package_writes(&backward(5), None, None, &result).expect("paginate");

        assert_eq!(
            served(&page),
            vec![(11, 1, c, 1), (11, 0, b, 1), (10, 0, a, 1)]
        );
        assert_eq!(page.start, Some(tx(10, 0)));
        assert_eq!(page.end, Some(tx(11, 1)));
        assert!(!page.has_previous_page);
        assert!(!page.has_next_page);
    }

    /// A `before` cursor's transaction is re-included; the writes it already covered are skipped.
    #[test]
    fn backward_before_tx_skips_covered_writes() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        let result = StreamPage::for_test(
            vec![pkg_item(11, &[(b, 1), (c, 1)]), pkg_item(10, &[(a, 1)])],
            None,
            None,
            Some(v2::QueryEndReason::LedgerTip),
        );

        let page = paginate_package_writes(
            &backward_before(5, tx(11, 1)),
            None,
            Some(tx(11, 1)),
            &result,
        )
        .expect("paginate");

        assert_eq!(served(&page), vec![(11, 0, b, 1), (10, 0, a, 1)]);
        assert!(page.has_next_page);
    }

    /// A page that served nothing still reports scan progress through its fences.
    #[test]
    fn empty_page_keeps_scan_fences() {
        let result: StreamPage<v2::ExecutedTransaction> = StreamPage::for_test(
            vec![],
            Some(wire_cursor(6, 8)),
            Some(wire_cursor(9, 15)),
            None,
        );

        let page = paginate_package_writes(&forward(5), None, None, &result).expect("paginate");

        assert!(page.writes.is_empty());
        assert_eq!(page.start, Some(scan(6, 8)));
        assert_eq!(page.end, Some(scan(9, 15)));
        assert!(page.has_next_page);
    }

    #[test]
    fn scan_request_bounds_and_ordering() {
        let request = build_scan_request(&forward(5), &(3..=9), None, None);
        assert_eq!(request.start_checkpoint, Some(3));
        // `cp_bounds` end is inclusive; the request bound is exclusive.
        assert_eq!(request.end_checkpoint, Some(10));

        let options = request.options.expect("options");
        // One extra transaction beyond the page limit for a re-included boundary transaction.
        assert_eq!(options.limit, Some(6));
        assert_eq!(options.ordering, Some(v2::Ordering::Ascending as i32));
        assert_eq!(options.after, None);
        assert_eq!(options.before, None);

        let request = build_scan_request(&backward(5), &(3..=9), None, None);
        let options = request.options.expect("options");
        assert_eq!(options.ordering, Some(v2::Ordering::Descending as i32));
    }

    /// A `Scan` after-bound excludes its transaction directly, hint intact.
    #[test]
    fn scan_request_after_scan_bounds_directly() {
        let request = build_scan_request(&forward(5), &(0..=9), Some(&scan(4, 17)), None);
        let after = request.options.expect("options").after.expect("after");
        assert_eq!(wire_position(&after), (4, 17));
    }

    /// A `Tx` after-bound widens to re-include its transaction, hint reset to unknown.
    #[test]
    fn scan_request_after_tx_widens() {
        let request = build_scan_request(&forward(5), &(0..=9), Some(&tx(17, 2)), None);
        let after = request.options.expect("options").after.expect("after");
        assert_eq!(wire_position(&after), (0, 16));
    }

    /// Nothing precedes the first transaction: a `Tx` after-bound at `tx_seq` 0 resumes from the
    /// range start.
    #[test]
    fn scan_request_after_tx_zero_unbounded() {
        let request = build_scan_request(&forward(5), &(0..=9), Some(&tx(0, 2)), None);
        assert_eq!(request.options.expect("options").after, None);
    }

    /// `Scan` and first-write `Tx` before-bounds exclude their transaction directly, hint intact;
    /// a 0 hint is treated as unknown.
    #[test]
    fn scan_request_before_bounds_directly() {
        let request = build_scan_request(&forward(5), &(0..=9), None, Some(&scan(4, 17)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (4, 17));

        let request = build_scan_request(&forward(5), &(0..=9), None, Some(&tx(17, 0)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (CP, 17));

        let request = build_scan_request(&forward(5), &(0..=9), None, Some(&scan(0, 17)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (u64::MAX, 17));
    }

    /// A mid-transaction `Tx` before-bound widens to re-include its transaction.
    #[test]
    fn scan_request_before_tx_widens() {
        let request = build_scan_request(&forward(5), &(0..=9), None, Some(&tx(17, 3)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (u64::MAX, 18));
    }
}
