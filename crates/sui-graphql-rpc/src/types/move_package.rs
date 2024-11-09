// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::balance::{self, Balance};
use super::base64::Base64;
use super::big_int::BigInt;
use super::coin::Coin;
use super::cursor::{BcsCursor, JsonCursor, Page, RawPaginated, ScanLimited, Target};
use super::move_module::MoveModule;
use super::move_object::MoveObject;
use super::object::{self, Object, ObjectFilter, ObjectImpl, ObjectOwner, ObjectStatus};
use super::owner::OwnerImpl;
use super::stake::StakedSui;
use super::sui_address::SuiAddress;
use super::suins_registration::{DomainFormat, SuinsRegistration};
use super::transaction_block::{self, TransactionBlock, TransactionBlockFilter};
use super::type_filter::ExactTypeFilter;
use super::uint53::UInt53;
use crate::connection::ScanConnection;
use crate::consistency::{Checkpointed, ConsistentNamedCursor};
use crate::data::{DataLoader, Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::raw_query::RawQuery;
use crate::types::sui_address::addr;
use crate::{filter, query};
use async_graphql::connection::{Connection, CursorType, Edge};
use async_graphql::dataloader::Loader;
use async_graphql::*;
use diesel::prelude::QueryableByName;
use diesel::{BoolExpressionMethods, ExpressionMethods, JoinOnDsl, QueryDsl, Selectable};
use diesel_async::scoped_futures::ScopedFutureExt;
use serde::{Deserialize, Serialize};
use sui_indexer::models::objects::StoredFullHistoryObject;
use sui_indexer::schema::packages;
use sui_package_resolver::{error::Error as PackageCacheError, Package as ParsedMovePackage};
use sui_types::is_system_package;
use sui_types::{move_package::MovePackage as NativeMovePackage, object::Data};

#[derive(Clone)]
pub(crate) struct MovePackage {
    /// Representation of this Move Object as a generic Object.
    pub super_: Object,

    /// Move-object-specific data, extracted from the native representation at
    /// `graphql_object.native_object.data`.
    pub native: NativeMovePackage,
}

/// Filter for paginating `MovePackage`s that were created within a range of checkpoints.
#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct MovePackageCheckpointFilter {
    /// Fetch packages that were published strictly after this checkpoint. Omitting this fetches
    /// packages published since genesis.
    pub after_checkpoint: Option<UInt53>,

    /// Fetch packages that were published strictly before this checkpoint. Omitting this fetches
    /// packages published up to the latest checkpoint (inclusive).
    pub before_checkpoint: Option<UInt53>,
}

/// Filter for paginating versions of a given `MovePackage`.
#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct MovePackageVersionFilter {
    /// Fetch versions of this package that are strictly newer than this version. Omitting this
    /// fetches versions since the original version.
    pub after_version: Option<UInt53>,

    /// Fetch versions of this package that are strictly older than this version. Omitting this
    /// fetches versions up to the latest version (inclusive).
    pub before_version: Option<UInt53>,
}

/// Filter for a point query of a MovePackage, supporting querying different versions of a package
/// by their version. Note that different versions of the same user package exist at different IDs
/// to each other, so this is different from looking up the historical version of an object.
pub(crate) enum PackageLookup {
    /// Get the package at the given address, if it was created before the given checkpoint.
    ById { checkpoint_viewed_at: u64 },

    /// Get the package whose original ID matches the storage ID of the package at the given
    /// address, but whose version is `version`.
    Versioned {
        version: u64,
        checkpoint_viewed_at: u64,
    },

    /// Get the package whose original ID matches the storage ID of the package at the given
    /// address, but that has the max version at the given checkpoint.
    Latest { checkpoint_viewed_at: u64 },
}

/// Information used by a package to link to a specific version of its dependency.
#[derive(SimpleObject)]
struct Linkage {
    /// The ID on-chain of the first version of the dependency.
    original_id: SuiAddress,

    /// The ID on-chain of the version of the dependency that this package depends on.
    upgraded_id: SuiAddress,

    /// The version of the dependency that this package depends on.
    version: UInt53,
}

/// Information about which previous versions of a package introduced its types.
#[derive(SimpleObject)]
struct TypeOrigin {
    /// Module defining the type.
    module: String,

    /// Name of the struct.
    #[graphql(name = "struct")]
    struct_: String,

    /// The storage ID of the package that first defined this type.
    defining_id: SuiAddress,
}

/// A wrapper around the stored representation of a package, used to implement pagination-related
/// traits.
#[derive(Selectable, QueryableByName)]
#[diesel(table_name = packages)]
struct StoredHistoryPackage {
    checkpoint_sequence_number: i64,
    original_id: Vec<u8>,
    #[diesel(embed)]
    object: StoredFullHistoryObject,
}

pub(crate) struct MovePackageDowncastError;

pub(crate) type CModule = JsonCursor<ConsistentNamedCursor>;
pub(crate) type Cursor = BcsCursor<PackageCursor>;

/// The inner struct for the `MovePackage` cursor. The package is identified by the checkpoint it
/// was created in, its original ID, and its version, and the `checkpoint_viewed_at` specifies the
/// checkpoint snapshot that the data came from.
///
/// The cursor includes the checkpoint the package was created in as well, so that when we paginate
/// through all the packages on-chain, if we pause half way through, we can pick back up based on
/// the checkpoint we've seen so far.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct PackageCursor {
    pub checkpoint_sequence_number: u64,
    pub original_id: Vec<u8>,
    pub package_version: u64,
    pub checkpoint_viewed_at: u64,
}

/// `DataLoader` key for fetching the storage ID of the (user) package that shares an original (aka
/// runtime) ID with the package stored at `package_id`, and whose version is `version`.
///
/// Note that this is different from looking up the historical version of an object -- the query
/// returns the ID of the package (each version of a user package is at a different ID) -- and it
/// does not work for system packages (whose versions do all reside under the same ID).
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct PackageVersionKey {
    address: SuiAddress,
    version: u64,
}

/// `DataLoader` key for fetching the latest version of a user package: The package with the largest
/// version whose original ID matches the original ID of the package at `address`.
#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct LatestKey {
    address: SuiAddress,
    checkpoint_viewed_at: u64,
}

/// A MovePackage is a kind of Move object that represents code that has been published on chain.
/// It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
    pub(crate) async fn address(&self) -> SuiAddress {
        OwnerImpl::from(&self.super_).address().await
    }

    /// Objects owned by this package, optionally `filter`-ed.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, MoveObject>> {
        OwnerImpl::from(&self.super_)
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// Total balance of all coins with marker type owned by this package. If type is not supplied,
    /// it defaults to `0x2::sui::SUI`.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Option<Balance>> {
        OwnerImpl::from(&self.super_).balance(ctx, type_).await
    }

    /// The balances of all coin types owned by this package.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Connection<String, Balance>> {
        OwnerImpl::from(&self.super_)
            .balances(ctx, first, after, last, before)
            .await
    }

    /// The coin objects owned by this package.
    ///
    ///`type` is a filter on the coin's type parameter, defaulting to `0x2::sui::SUI`.
    ///
    /// Note that coins owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn coins(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
        type_: Option<ExactTypeFilter>,
    ) -> Result<Connection<String, Coin>> {
        OwnerImpl::from(&self.super_)
            .coins(ctx, first, after, last, before, type_)
            .await
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by this package.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn staked_suis(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, StakedSui>> {
        OwnerImpl::from(&self.super_)
            .staked_suis(ctx, first, after, last, before)
            .await
    }

    /// The domain explicitly configured as the default domain pointing to this object.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
        format: Option<DomainFormat>,
    ) -> Result<Option<String>> {
        OwnerImpl::from(&self.super_)
            .default_suins_name(ctx, format)
            .await
    }

    /// The SuinsRegistration NFTs owned by this package. These grant the owner the capability to
    /// manage the associated domain.
    ///
    /// Note that objects owned by a package are inaccessible, because packages are immutable and
    /// cannot be owned by an address.
    pub(crate) async fn suins_registrations(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<object::Cursor>,
        last: Option<u64>,
        before: Option<object::Cursor>,
    ) -> Result<Connection<String, SuinsRegistration>> {
        OwnerImpl::from(&self.super_)
            .suins_registrations(ctx, first, after, last, before)
            .await
    }

    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl(&self.super_).version().await
    }

    /// The current status of the object as read from the off-chain store. The possible states are:
    /// NOT_INDEXED, the object is loaded from serialized data, such as the contents of a genesis or
    /// system package upgrade transaction. LIVE, the version returned is the most recent for the
    /// object, and it is not deleted or wrapped at that version. HISTORICAL, the object was
    /// referenced at a specific version or checkpoint, so is fetched from historical tables and may
    /// not be the latest version of the object. WRAPPED_OR_DELETED, the object is deleted or
    /// wrapped and only partial information can be loaded."
    pub(crate) async fn status(&self) -> ObjectStatus {
        ObjectImpl(&self.super_).status().await
    }

    /// 32-byte hash that identifies the package's contents, encoded as a Base58 string.
    pub(crate) async fn digest(&self) -> Option<String> {
        ObjectImpl(&self.super_).digest().await
    }

    /// The owner type of this object: Immutable, Shared, Parent, Address
    /// Packages are always Immutable.
    pub(crate) async fn owner(&self) -> Option<ObjectOwner> {
        ObjectImpl(&self.super_).owner().await
    }

    /// The transaction block that published or upgraded this package.
    pub(crate) async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        ObjectImpl(&self.super_)
            .previous_transaction_block(ctx)
            .await
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated. This number is
    /// recalculated based on the present storage gas price.
    ///
    /// Note that packages cannot be deleted or mutated, so this number is provided purely for
    /// reference.
    pub(crate) async fn storage_rebate(&self) -> Option<BigInt> {
        ObjectImpl(&self.super_).storage_rebate().await
    }

    /// The transaction blocks that sent objects to this package.
    ///
    /// Note that objects that have been sent to a package become inaccessible.
    ///
    /// `scanLimit` restricts the number of candidate transactions scanned when gathering a page of
    /// results. It is required for queries that apply more than two complex filters (on function,
    /// kind, sender, recipient, input object, changed object, or ids), and can be at most
    /// `serviceConfig.maxScanLimit`.
    ///
    /// When the scan limit is reached the page will be returned even if it has fewer than `first`
    /// results when paginating forward (`last` when paginating backwards). If there are more
    /// transactions to scan, `pageInfo.hasNextPage` (or `pageInfo.hasPreviousPage`) will be set to
    /// `true`, and `PageInfo.endCursor` (or `PageInfo.startCursor`) will be set to the last
    /// transaction that was scanned as opposed to the last (or first) transaction in the page.
    ///
    /// Requesting the next (or previous) page after this cursor will resume the search, scanning
    /// the next `scanLimit` many transactions in the direction of pagination, and so on until all
    /// transactions in the scanning range have been visited.
    ///
    /// By default, the scanning range includes all transactions known to GraphQL, but it can be
    /// restricted by the `after` and `before` cursors, and the `beforeCheckpoint`,
    /// `afterCheckpoint` and `atCheckpoint` filters.
    pub(crate) async fn received_transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
        scan_limit: Option<u64>,
    ) -> Result<ScanConnection<String, TransactionBlock>> {
        ObjectImpl(&self.super_)
            .received_transaction_blocks(ctx, first, after, last, before, filter, scan_limit)
            .await
    }

    /// The Base64-encoded BCS serialization of the package's content.
    pub(crate) async fn bcs(&self) -> Result<Option<Base64>> {
        ObjectImpl(&self.super_).bcs().await
    }

    /// Fetch another version of this package (the package that shares this package's original ID,
    /// but has the specified `version`).
    async fn package_at_version(
        &self,
        ctx: &Context<'_>,
        version: u64,
    ) -> Result<Option<MovePackage>> {
        MovePackage::query(
            ctx,
            self.super_.address,
            MovePackage::by_version(version, self.checkpoint_viewed_at_impl()),
        )
        .await
        .extend()
    }

    /// Fetch all versions of this package (packages that share this package's original ID),
    /// optionally bounding the versions exclusively from below with `afterVersion`, or from above
    /// with `beforeVersion`.
    async fn package_versions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<Cursor>,
        last: Option<u64>,
        before: Option<Cursor>,
        filter: Option<MovePackageVersionFilter>,
    ) -> Result<Connection<String, MovePackage>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        MovePackage::paginate_by_version(
            ctx.data_unchecked(),
            page,
            self.super_.address,
            filter,
            self.checkpoint_viewed_at_impl(),
        )
        .await
        .extend()
    }

    /// Fetch the latest version of this package (the package with the highest `version` that shares
    /// this packages's original ID)
    async fn latest_package(&self, ctx: &Context<'_>) -> Result<MovePackage> {
        Ok(MovePackage::query(
            ctx,
            self.super_.address,
            MovePackage::latest_at(self.checkpoint_viewed_at_impl()),
        )
        .await
        .extend()?
        .ok_or_else(|| Error::Internal("No latest version found".to_string()))?)
    }

    /// A representation of the module called `name` in this package, including the
    /// structs and functions it defines.
    async fn module(&self, name: String) -> Result<Option<MoveModule>> {
        self.module_impl(&name).extend()
    }

    /// Paginate through the MoveModules defined in this package.
    pub async fn modules(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CModule>,
        last: Option<u64>,
        before: Option<CModule>,
    ) -> Result<Option<Connection<String, MoveModule>>> {
        use std::ops::Bound as B;

        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at =
            cursor_viewed_at.unwrap_or_else(|| self.checkpoint_viewed_at_impl());

        let parsed = self.parsed_package()?;
        let module_range = parsed.modules().range::<String, _>((
            page.after().map_or(B::Unbounded, |a| B::Excluded(&a.name)),
            page.before().map_or(B::Unbounded, |b| B::Excluded(&b.name)),
        ));

        let mut connection = Connection::new(false, false);
        let modules = if page.is_from_front() {
            module_range.take(page.limit()).collect()
        } else {
            let mut ms: Vec<_> = module_range.rev().take(page.limit()).collect();
            ms.reverse();
            ms
        };

        connection.has_previous_page = modules.first().is_some_and(|(fst, _)| {
            parsed
                .modules()
                .range::<String, _>((B::Unbounded, B::Excluded(*fst)))
                .next()
                .is_some()
        });

        connection.has_next_page = modules.last().is_some_and(|(lst, _)| {
            parsed
                .modules()
                .range::<String, _>((B::Excluded(*lst), B::Unbounded))
                .next()
                .is_some()
        });

        for (name, parsed) in modules {
            let Some(native) = self.native.serialized_module_map().get(name) else {
                return Err(Error::Internal(format!(
                    "Module '{name}' exists in PackageCache but not in serialized map.",
                ))
                .extend());
            };

            let cursor = JsonCursor::new(ConsistentNamedCursor {
                name: name.clone(),
                c: checkpoint_viewed_at,
            })
            .encode_cursor();
            connection.edges.push(Edge::new(
                cursor,
                MoveModule {
                    storage_id: self.super_.address,
                    native: native.clone(),
                    parsed: parsed.clone(),
                    checkpoint_viewed_at,
                },
            ))
        }

        if connection.edges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(connection))
        }
    }

    /// The transitive dependencies of this package.
    async fn linkage(&self) -> Option<Vec<Linkage>> {
        let linkage = self
            .native
            .linkage_table()
            .iter()
            .map(|(&runtime_id, upgrade_info)| Linkage {
                original_id: runtime_id.into(),
                upgraded_id: upgrade_info.upgraded_id.into(),
                version: upgrade_info.upgraded_version.value().into(),
            })
            .collect();

        Some(linkage)
    }

    /// The (previous) versions of this package that introduced its types.
    async fn type_origins(&self) -> Option<Vec<TypeOrigin>> {
        let type_origins = self
            .native
            .type_origin_table()
            .iter()
            .map(|origin| TypeOrigin {
                module: origin.module_name.clone(),
                struct_: origin.datatype_name.clone(),
                defining_id: origin.package.into(),
            })
            .collect();

        Some(type_origins)
    }

    /// BCS representation of the package itself, as a MovePackage.
    async fn package_bcs(&self) -> Result<Option<Base64>> {
        let bcs = bcs::to_bytes(&self.native)
            .map_err(|_| {
                Error::Internal(format!("Failed to serialize package {}", self.native.id()))
            })
            .extend()?;

        Ok(Some(bcs.into()))
    }

    /// BCS representation of the package's modules.  Modules appear as a sequence of pairs (module
    /// name, followed by module bytes), in alphabetic order by module name.
    async fn module_bcs(&self) -> Result<Option<Base64>> {
        let bcs = bcs::to_bytes(self.native.serialized_module_map())
            .map_err(|_| {
                Error::Internal(format!("Failed to serialize package {}", self.native.id()))
            })
            .extend()?;

        Ok(Some(bcs.into()))
    }
}

impl MovePackage {
    fn parsed_package(&self) -> Result<ParsedMovePackage, Error> {
        ParsedMovePackage::read_from_package(&self.native)
            .map_err(|e| Error::Internal(format!("Error reading package: {e}")))
    }

    /// This package was viewed at a snapshot of the chain state at this checkpoint (identified by
    /// its sequence number).
    fn checkpoint_viewed_at_impl(&self) -> u64 {
        self.super_.checkpoint_viewed_at
    }

    pub(crate) fn module_impl(&self, name: &str) -> Result<Option<MoveModule>, Error> {
        use PackageCacheError as E;
        match (
            self.native.serialized_module_map().get(name),
            self.parsed_package()?.module(name),
        ) {
            (Some(native), Ok(parsed)) => Ok(Some(MoveModule {
                storage_id: self.super_.address,
                native: native.clone(),
                parsed: parsed.clone(),
                checkpoint_viewed_at: self.checkpoint_viewed_at_impl(),
            })),

            (None, _) | (_, Err(E::ModuleNotFound(_, _))) => Ok(None),
            (_, Err(e)) => Err(Error::Internal(format!(
                "Unexpected error fetching module: {e}"
            ))),
        }
    }

    /// Look-up the package by its Storage ID, as of a given checkpoint.
    pub(crate) fn by_id_at(checkpoint_viewed_at: u64) -> PackageLookup {
        PackageLookup::ById {
            checkpoint_viewed_at,
        }
    }

    /// Look-up a specific version of the package, identified by the storage ID of any version of
    /// the package, and the desired version (the actual object loaded might be at a different
    /// object ID).
    pub(crate) fn by_version(version: u64, checkpoint_viewed_at: u64) -> PackageLookup {
        PackageLookup::Versioned {
            version,
            checkpoint_viewed_at,
        }
    }

    /// Look-up the package that shares the same original ID as the package at `address`, but has
    /// the latest version, as of the given checkpoint.
    pub(crate) fn latest_at(checkpoint_viewed_at: u64) -> PackageLookup {
        PackageLookup::Latest {
            checkpoint_viewed_at,
        }
    }

    pub(crate) async fn query(
        ctx: &Context<'_>,
        address: SuiAddress,
        key: PackageLookup,
    ) -> Result<Option<Self>, Error> {
        let (address, key) = match key {
            PackageLookup::ById {
                checkpoint_viewed_at,
            } => (address, Object::latest_at(checkpoint_viewed_at)),

            PackageLookup::Versioned {
                version,
                checkpoint_viewed_at,
            } => {
                if is_system_package(address) {
                    (address, Object::at_version(version, checkpoint_viewed_at))
                } else {
                    let DataLoader(loader) = &ctx.data_unchecked();
                    let Some(translation) = loader
                        .load_one(PackageVersionKey { address, version })
                        .await?
                    else {
                        return Ok(None);
                    };

                    (translation, Object::latest_at(checkpoint_viewed_at))
                }
            }

            PackageLookup::Latest {
                checkpoint_viewed_at,
            } => {
                if is_system_package(address) {
                    (address, Object::latest_at(checkpoint_viewed_at))
                } else {
                    let DataLoader(loader) = &ctx.data_unchecked();
                    let Some(translation) = loader
                        .load_one(LatestKey {
                            address,
                            checkpoint_viewed_at,
                        })
                        .await?
                    else {
                        return Ok(None);
                    };

                    (translation, Object::latest_at(checkpoint_viewed_at))
                }
            }
        };

        let Some(object) = Object::query(ctx, address, key).await? else {
            return Ok(None);
        };

        Ok(Some(MovePackage::try_from(&object).map_err(|_| {
            Error::Internal(format!("{address} is not a package"))
        })?))
    }

    /// Query the database for a `page` of Move packages. The Page uses the checkpoint sequence
    /// number the package was created at, its original ID, and its version as the cursor. The query
    /// can optionally be filtered by a bound on the checkpoints the packages were created in.
    ///
    /// The `checkpoint_viewed_at` parameter represents the checkpoint sequence number at which this
    /// page was queried. Each entity returned in the connection will inherit this checkpoint, so
    /// that when viewing that entity's state, it will be as if it is being viewed at this
    /// checkpoint.
    ///
    /// The cursors in `page` may also include checkpoint viewed at fields. If these are set, they
    /// take precedence over the checkpoint that pagination is being conducted in.
    pub(crate) async fn paginate_by_checkpoint(
        db: &Db,
        page: Page<Cursor>,
        filter: Option<MovePackageCheckpointFilter>,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, MovePackage>, Error> {
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);

        let after_checkpoint: Option<u64> = filter
            .as_ref()
            .and_then(|f| f.after_checkpoint)
            .map(|v| v.into());

        // Clamp the "before checkpoint" bound by "checkpoint viewed at".
        let before_checkpoint = filter
            .as_ref()
            .and_then(|f| f.before_checkpoint)
            .map(|v| v.into())
            .unwrap_or(u64::MAX)
            .min(checkpoint_viewed_at + 1);

        let (prev, next, results) = db
            .execute(move |conn| {
                async move {
                    let mut q = query!(
                        r#"
                            SELECT
                                    p.checkpoint_sequence_number,
                                    p.original_id,
                                    o.*
                            FROM
                                    packages p
                            INNER JOIN
                                    full_objects_history o
                            ON
                                    p.package_id = o.object_id
                            AND     p.package_version = o.object_version
                        "#
                    );

                    q = filter!(
                        q,
                        format!("p.checkpoint_sequence_number < {before_checkpoint}")
                    );
                    if let Some(after) = after_checkpoint {
                        q = filter!(q, format!("{after} < p.checkpoint_sequence_number"));
                    }

                    page.paginate_raw_query::<StoredHistoryPackage>(conn, checkpoint_viewed_at, q)
                        .await
                }
                .scope_boxed()
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        // The "checkpoint viewed at" sets a consistent upper bound for the nested queries.
        for stored in results {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            let package = MovePackage::try_from_serialized(stored.object, checkpoint_viewed_at)?;
            conn.edges.push(Edge::new(cursor, package));
        }

        Ok(conn)
    }

    /// Query the database for a `page` of Move packages. The Page uses the checkpoint sequence
    /// number the package was created at, its original ID, and its version as the cursor. The query
    /// is filtered by the ID of a package and will only return packages from the same family
    /// (sharing the same original ID as the package whose ID was given), and can optionally be
    /// filtered by an upper and lower bound on package version.
    ///
    /// The `checkpoint_viewed_at` parameter represents the checkpoint sequence number at which this
    /// page was queried. Each entity returned in the connection will inherit this checkpoint, so
    /// that when viewing that entity's state, it will be as if it is being viewed at this
    /// checkpoint.
    ///
    /// The cursors in `page` may also include checkpoint viewed at fields. If these are set, they
    /// take precedence over the checkpoint that pagination is being conducted in.
    pub(crate) async fn paginate_by_version(
        db: &Db,
        page: Page<Cursor>,
        package: SuiAddress,
        filter: Option<MovePackageVersionFilter>,
        checkpoint_viewed_at: u64,
    ) -> Result<Connection<String, MovePackage>, Error> {
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at = cursor_viewed_at.unwrap_or(checkpoint_viewed_at);
        let (prev, next, results) = db
            .execute(move |conn| {
                async move {
                    page.paginate_raw_query::<StoredHistoryPackage>(
                        conn,
                        checkpoint_viewed_at,
                        if is_system_package(package) {
                            system_package_version_query(package, filter)
                        } else {
                            user_package_version_query(package, filter)
                        },
                    )
                    .await
                }
                .scope_boxed()
            })
            .await?;

        let mut conn = Connection::new(prev, next);

        // The "checkpoint viewed at" sets a consistent upper bound for the nested queries.
        for stored in results {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            let package = MovePackage::try_from_serialized(stored.object, checkpoint_viewed_at)?;
            conn.edges.push(Edge::new(cursor, package));
        }

        Ok(conn)
    }

    /// `checkpoint_viewed_at` points to the checkpoint snapshot that this `MovePackage` came from.
    /// This is stored in the `MovePackage` so that related fields from the package are read from
    /// the same checkpoint (consistently).
    pub(crate) fn try_from_serialized(
        history_object: StoredFullHistoryObject,
        checkpoint_viewed_at: u64,
    ) -> Result<Self, Error> {
        let object = Object::new_serialized(
            SuiAddress::from_bytes(&history_object.object_id)
                .map_err(|_| Error::Internal("Invalid package ID".to_string()))?,
            history_object.object_version as u64,
            history_object.serialized_object,
            checkpoint_viewed_at,
            history_object.object_version as u64,
        )
        .ok_or_else(|| Error::Internal("Not a package!".to_string()))?;

        Self::try_from(&object).map_err(|_| Error::Internal("Not a package!".to_string()))
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}

impl RawPaginated<Cursor> for StoredHistoryPackage {
    fn filter_ge(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!(
                "(p.checkpoint_sequence_number > {cp} OR (\
                  p.checkpoint_sequence_number = {cp} AND \
                 (original_id > '\\x{id}'::bytea OR (\
                  original_id = '\\x{id}'::bytea AND \
                  object_version >= {pv}\
                 ))))",
                cp = cursor.checkpoint_sequence_number,
                id = hex::encode(&cursor.original_id),
                pv = cursor.package_version,
            )
        )
    }

    fn filter_le(cursor: &Cursor, query: RawQuery) -> RawQuery {
        filter!(
            query,
            format!(
                "(p.checkpoint_sequence_number < {cp} OR (\
                  p.checkpoint_sequence_number = {cp} AND \
                 (original_id < '\\x{id}'::bytea OR (\
                  original_id = '\\x{id}'::bytea AND \
                  object_version <= {pv}\
                 ))))",
                cp = cursor.checkpoint_sequence_number,
                id = hex::encode(&cursor.original_id),
                pv = cursor.package_version,
            )
        )
    }

    fn order(asc: bool, query: RawQuery) -> RawQuery {
        if asc {
            query
                .order_by("1 ASC") // checkpoint_sequence_number
                .order_by("2 ASC") // original_id
                .order_by("object_version ASC")
        } else {
            query
                .order_by("1 DESC") // checkpoint_sequence_number
                .order_by("2 DESC") // original_id
                .order_by("object_version DESC")
        }
    }
}

impl Target<Cursor> for StoredHistoryPackage {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(PackageCursor {
            checkpoint_sequence_number: self.checkpoint_sequence_number as u64,
            original_id: self.original_id.clone(),
            package_version: self.object.object_version as u64,
            checkpoint_viewed_at,
        })
    }
}

impl ScanLimited for BcsCursor<PackageCursor> {}

#[async_trait::async_trait]
impl Loader<PackageVersionKey> for Db {
    type Value = SuiAddress;
    type Error = Error;

    async fn load(
        &self,
        keys: &[PackageVersionKey],
    ) -> Result<HashMap<PackageVersionKey, SuiAddress>, Error> {
        use packages::dsl;
        let other = diesel::alias!(packages as other);

        let id_versions: BTreeSet<_> = keys
            .iter()
            .map(|k| (k.address.into_vec(), k.version as i64))
            .collect();

        let stored_packages: Vec<(Vec<u8>, i64, Vec<u8>)> = self
            .execute(move |conn| {
                async move {
                    conn.results(|| {
                        let mut query = dsl::packages
                            .inner_join(
                                other.on(dsl::original_id.eq(other.field(dsl::original_id))),
                            )
                            .select((
                                dsl::package_id,
                                other.field(dsl::package_version),
                                other.field(dsl::package_id),
                            ))
                            .into_boxed();

                        for (id, version) in id_versions.iter().cloned() {
                            query = query.or_filter(
                                dsl::package_id
                                    .eq(id)
                                    .and(other.field(dsl::package_version).eq(version)),
                            );
                        }

                        query
                    })
                    .await
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to load packages: {e}")))?;

        let mut result = HashMap::new();
        for (id, version, other_id) in stored_packages {
            result.insert(
                PackageVersionKey {
                    address: addr(&id)?,
                    version: version as u64,
                },
                addr(&other_id)?,
            );
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl Loader<LatestKey> for Db {
    type Value = SuiAddress;
    type Error = Error;

    async fn load(&self, keys: &[LatestKey]) -> Result<HashMap<LatestKey, SuiAddress>, Error> {
        use packages::dsl;
        let other = diesel::alias!(packages as other);

        let mut ids_by_cursor: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        for key in keys {
            ids_by_cursor
                .entry(key.checkpoint_viewed_at)
                .or_default()
                .insert(key.address.into_vec());
        }

        // Issue concurrent reads for each group of IDs
        let futures = ids_by_cursor
            .into_iter()
            .map(|(checkpoint_viewed_at, ids)| {
                self.execute(move |conn| {
                    async move {
                        let results: Vec<(Vec<u8>, Vec<u8>)> = conn
                            .results(|| {
                                let o_original_id = other.field(dsl::original_id);
                                let o_package_id = other.field(dsl::package_id);
                                let o_cp_seq_num = other.field(dsl::checkpoint_sequence_number);
                                let o_version = other.field(dsl::package_version);

                                let query = dsl::packages
                                    .inner_join(other.on(dsl::original_id.eq(o_original_id)))
                                    .select((dsl::package_id, o_package_id))
                                    .filter(dsl::package_id.eq_any(ids.iter().cloned()))
                                    .filter(o_cp_seq_num.le(checkpoint_viewed_at as i64))
                                    .order_by((dsl::package_id, dsl::original_id, o_version.desc()))
                                    .distinct_on((dsl::package_id, dsl::original_id));
                                query
                            })
                            .await?;

                        Ok::<_, diesel::result::Error>(
                            results
                                .into_iter()
                                .map(|(p, latest)| (checkpoint_viewed_at, p, latest))
                                .collect::<Vec<_>>(),
                        )
                    }
                    .scope_boxed()
                })
            });

        // Wait for the reads to all finish, and gather them into the result map.
        let groups = futures::future::join_all(futures).await;

        let mut results = HashMap::new();
        for group in groups {
            for (checkpoint_viewed_at, address, latest) in
                group.map_err(|e| Error::Internal(format!("Failed to fetch packages: {e}")))?
            {
                results.insert(
                    LatestKey {
                        address: addr(&address)?,
                        checkpoint_viewed_at,
                    },
                    addr(&latest)?,
                );
            }
        }

        Ok(results)
    }
}

impl TryFrom<&Object> for MovePackage {
    type Error = MovePackageDowncastError;

    fn try_from(object: &Object) -> Result<Self, MovePackageDowncastError> {
        let Some(native) = object.native_impl() else {
            return Err(MovePackageDowncastError);
        };

        if let Data::Package(move_package) = &native.data {
            Ok(Self {
                super_: object.clone(),
                native: move_package.clone(),
            })
        } else {
            Err(MovePackageDowncastError)
        }
    }
}

/// Query for fetching all the versions of a system package (assumes that `package` has already been
/// verified as a system package). This is a `full_objects_history` query disguised as a package query.
///
/// We do this because the `packages` table contains only one entry per package ID. For the system
/// packages, this is the latest version of the package (for user packages, there is only one entry
/// per package ID anyway as each version of a package gets its own ID).
fn system_package_version_query(
    package: SuiAddress,
    filter: Option<MovePackageVersionFilter>,
) -> RawQuery {
    // Query uses a left join to force the query planner to use `objects_version` in the outer loop.
    let mut q = query!(
        r#"
            SELECT
                    p.checkpoint_sequence_number,
                    p.original_id,
                    o.*
            FROM (
                    SELECT
                        object_id AS package_id,
                        object_id AS original_id,
                        object_version AS package_version,
                        cp_sequence_number AS checkpoint_sequence_number
                    FROM
                        objects_version
            ) p
            LEFT JOIN
                    full_objects_history o
            ON
                    p.package_id = o.object_id
            AND     p.package_version = o.object_version
        "#
    );

    q = filter!(
        q,
        format!(
            "original_id = '\\x{}'::bytea",
            hex::encode(package.into_vec())
        )
    );

    if let Some(after) = filter.as_ref().and_then(|f| f.after_version) {
        let a: u64 = after.into();
        q = filter!(q, format!("object_version > {a}"));
    }

    if let Some(before) = filter.as_ref().and_then(|f| f.before_version) {
        let b: u64 = before.into();
        q = filter!(q, format!("object_version < {b}"));
    }

    q
}

/// Query for fetching all the versions of a non-system package (assumes that `package` has already
/// been verified as a system package)
fn user_package_version_query(
    package: SuiAddress,
    filter: Option<MovePackageVersionFilter>,
) -> RawQuery {
    let mut q = query!(
        r#"
            SELECT
                    p.checkpoint_sequence_number,
                    p.original_id,
                    o.*
            FROM
                    packages q
            INNER JOIN
                    packages p
            USING  (original_id)
            INNER JOIN
                    full_objects_history o
            ON
                    p.package_id = o.object_id
            AND     p.package_version = o.object_version
        "#
    );

    q = filter!(
        q,
        format!(
            "q.package_id = '\\x{}'::bytea",
            hex::encode(package.into_vec())
        )
    );

    if let Some(after) = filter.as_ref().and_then(|f| f.after_version) {
        let a: u64 = after.into();
        q = filter!(q, format!("p.package_version > {a}"));
    }

    if let Some(before) = filter.as_ref().and_then(|f| f.before_version) {
        let b: u64 = before.into();
        q = filter!(q, format!("p.package_version < {b}"));
    }

    q
}
