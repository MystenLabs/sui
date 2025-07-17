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
use sui_pg_db::sql;
use sui_sql_macro::query;
use sui_types::{
    base_types::{ObjectID, SuiAddress as NativeSuiAddress},
    move_package::MovePackage as NativeMovePackage,
    object::Object as NativeObject,
};

use crate::{
    api::scalars::{
        base64::Base64,
        cursor::{BcsCursor, JsonCursor},
        sui_address::SuiAddress,
        uint53::UInt53,
    },
    error::{bad_user_input, RpcError},
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    addressable::AddressableImpl,
    object::{self, CVersion, Object, ObjectImpl, VersionFilter},
    transaction::Transaction,
};

pub(crate) struct MovePackage {
    /// Representation of this Move Package as a generic Object.
    super_: Object,

    /// Move package specific data, extracted from the native representation of the generic object.
    contents: NativeMovePackage,
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
pub(crate) struct CheckpointFilter {
    /// Filter to packages that were published strictly after this checkpoint, defaults to fetching from the earliest checkpoint known to this RPC (this could be the genesis checkpoint, or some later checkpoint if data has been pruned).
    pub(crate) after_checkpoint: Option<UInt53>,

    /// Filter to packages published strictly before this checkpoint, defaults to fetching up to the latest checkpoint (inclusive).
    pub(crate) before_checkpoint: Option<UInt53>,
}

/// Inner struct for the cursor produced while iterating over all package publishes.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub(crate) struct PackageCursor {
    pub original_id: Vec<u8>,
    pub cp_sequence_number: u64,
    pub package_version: u64,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error(
        "At most one of a version, or a checkpoint bound can be specified when fetching a package"
    )]
    OneBound,
}

/// Cursor for iterating over package publishes. Points to the publish of a particular
/// version of a package, in a given checkpoint.
pub(crate) type CPackage = BcsCursor<PackageCursor>;

/// Cursor for iterating over system packages. Points at a particular system package, by its ID.
pub(crate) type CSysPackage = BcsCursor<Vec<u8>>;

/// A MovePackage is a kind of Object that represents code that has been published on-chain. It exposes information about its modules, type definitions, functions, and dependencies.
#[Object]
impl MovePackage {
    /// The MovePackage's ID.
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(&self.super_.super_).address()
    }

    /// The version of this package that this content comes from.
    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl::from(&self.super_).version()
    }

    /// 32-byte hash that identifies the package's contents, encoded in Base58.
    pub(crate) async fn digest(&self) -> String {
        ObjectImpl::from(&self.super_).digest()
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
        ObjectImpl::from(&self.super_)
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this package, as an `Object`.
    pub(crate) async fn object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_).object_bcs(ctx).await
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
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
    async fn package_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let bytes = bcs::to_bytes(&self.contents).context("Failed to serialize MovePackage")?;
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
    ) -> Result<Connection<String, MovePackage>, RpcError<Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MovePackage", "packageVersionsAfter");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Apply any filter that was supplied to the query, but add an additional version
        // lowerbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            after_version: Some(self.super_.version.value().into()),
            ..VersionFilter::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        MovePackage::paginate_by_version(
            ctx,
            self.super_.super_.scope.clone(),
            page,
            self.super_.super_.address,
            filter,
        )
        .await
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
    ) -> Result<Connection<String, MovePackage>, RpcError<Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("MovePackage", "packageVersionsBefore");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Apply any filter that was supplied to the query, but add an additional version
        // upperbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            before_version: Some(self.super_.version.value().into()),
            ..VersionFilter::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        MovePackage::paginate_by_version(
            ctx,
            self.super_.super_.scope.clone(),
            page,
            self.super_.super_.address,
            filter,
        )
        .await
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
            .previous_transaction(ctx)
            .await
    }
}

impl MovePackage {
    /// Try to downcast an `Object` to a `MovePackage`. This function returns `None` if `object`'s
    /// contents cannot be fetched, or it is not a package.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let super_ = object.inflated(ctx).await?;

        let Some(super_contents) = &super_.contents else {
            return Ok(None);
        };

        let Some(contents) = super_contents.data.try_as_package().cloned() else {
            return Ok(None);
        };

        Ok(Some(Self { super_, contents }))
    }

    /// Fetch a package by its key. The key can either specify an exact version to fetch, an
    /// upperbound against a checkpoint, or neither.
    pub(crate) async fn by_key(
        ctx: &Context<'_>,
        scope: Scope,
        key: PackageKey,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let bounds = key.version.is_some() as u8 + key.at_checkpoint.is_some() as u8;

        if bounds > 1 {
            Err(bad_user_input(Error::OneBound))
        } else if let Some(v) = key.version {
            Ok(Self::at_version(ctx, scope, key.address, v).await?)
        } else if let Some(cp) = key.at_checkpoint {
            Ok(Self::checkpoint_bounded(ctx, scope, key.address, cp).await?)
        } else {
            let cp: UInt53 = scope.checkpoint_viewed_at().into();
            Ok(Self::checkpoint_bounded(ctx, scope, key.address, cp).await?)
        }
    }

    /// Fetch the package whose original ID matches the original ID of the package at `address`,
    /// but whose version is `version`.
    pub(crate) async fn at_version(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
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

        Self::from_stored(scope, stored_package)
    }

    /// Fetch the package whose original ID matches the original ID of the package at `address`,
    /// but whose version is latest among all packages that existed `at_checkpoint`.
    pub(crate) async fn checkpoint_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        at_checkpoint: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
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
    pub(crate) fn from_stored(
        scope: Scope,
        stored: StoredPackage,
    ) -> Result<Option<Self>, RpcError<Error>> {
        if stored.cp_sequence_number as u64 > scope.checkpoint_viewed_at() {
            return Ok(None);
        }

        let native: NativeObject = bcs::from_bytes(&stored.serialized_object)
            .context("Failed to deserialize package as object")?;

        let Some(contents) = native.data.try_as_package().cloned() else {
            return Ok(None);
        };

        let super_ = Object::from_contents(scope, Arc::new(native));
        Ok(Some(Self { super_, contents }))
    }

    /// Paginate through versions of a package, identified by its original ID. `address` points to
    /// any package on-chain that has that original ID.
    pub(crate) async fn paginate_by_version(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CVersion>,
        address: NativeSuiAddress,
        filter: VersionFilter,
    ) -> Result<Connection<String, MovePackage>, RpcError<Error>> {
        use kv_packages::dsl as p;

        let mut conn = Connection::new(false, false);

        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
        let pg_reader: &PgReader = ctx.data()?;

        let Some(original_id) = pg_loader
            .load_one(PackageOriginalIdKey(address.into()))
            .await
            .with_context(|| format!("Failed to fetch original package ID for {address}"))?
        else {
            // No original ID record for this package, so it either doesn't exist on chain, or it
            // is not a package.
            return Ok(conn);
        };

        // The original ID record exists but points to a package that is not visible at the
        // checkpoint being viewed.
        if original_id.cp_sequence_number as u64 > scope.checkpoint_viewed_at() {
            return Ok(conn);
        }

        let mut query = p::kv_packages
            .filter(p::cp_sequence_number.le(scope.checkpoint_viewed_at() as i64))
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

        let (prev, next, results) =
            page.paginate_results(results, |p| JsonCursor::new(p.package_version as u64));

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for (cursor, stored) in results {
            if let Some(object) = Self::from_stored(scope.clone(), stored)? {
                conn.edges.push(Edge::new(cursor.encode_cursor(), object));
            }
        }

        Ok(conn)
    }

    /// Paginate through all packages published in a range of checkpoints.
    pub(crate) async fn paginate_by_checkpoint(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CPackage>,
        filter: CheckpointFilter,
    ) -> Result<Connection<String, MovePackage>, RpcError<Error>> {
        use kv_packages::dsl as p;

        let mut conn = Connection::new(false, false);

        let pg_reader: &PgReader = ctx.data()?;

        let mut query = p::kv_packages
            .filter(p::cp_sequence_number.le(scope.checkpoint_viewed_at() as i64))
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

        let (prev, next, results) = page.paginate_results(results, |p| {
            BcsCursor::new(PackageCursor {
                original_id: p.original_id.clone(),
                cp_sequence_number: p.cp_sequence_number as u64,
                package_version: p.package_version as u64,
            })
        });

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for (cursor, stored) in results {
            if let Some(object) = Self::from_stored(scope.clone(), stored)? {
                conn.edges.push(Edge::new(cursor.encode_cursor(), object));
            }
        }

        Ok(conn)
    }

    /// Paginate through versions of a package, identified by its original ID. `address` points to
    /// any package on-chain that has that original ID.
    pub(crate) async fn paginate_system_packages(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CSysPackage>,
        checkpoint: u64,
    ) -> Result<Connection<String, MovePackage>, RpcError<Error>> {
        let mut conn = Connection::new(false, false);

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

        let (prev, next, results) =
            page.paginate_results(results, |p| BcsCursor::new(p.original_id.clone()));

        conn.has_previous_page = prev;
        conn.has_next_page = next;

        for (cursor, stored) in results {
            if let Some(object) = Self::from_stored(scope.clone(), stored)? {
                conn.edges.push(Edge::new(cursor.encode_cursor(), object));
            }
        }

        Ok(conn)
    }
}
