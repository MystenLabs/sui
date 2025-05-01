// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, Edge},
    dataloader::DataLoader,
    Context, InputObject, Interface, Object,
};
use diesel::{ExpressionMethods, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::KvLoader,
    object_versions::{
        CheckpointBoundedObjectVersionKey, VersionBoundedObjectVersionKey,
        VersionedObjectVersionKey,
    },
    pg_reader::PgReader,
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};
use sui_types::{
    base_types::{SequenceNumber, SuiAddress as NativeSuiAddress},
    digests::ObjectDigest,
    object::Object as NativeObject,
};
use tokio::join;

use crate::{
    api::scalars::{base64::Base64, cursor::JsonCursor, sui_address::SuiAddress, uint53::UInt53},
    error::{bad_user_input, RpcError},
    intersect,
    pagination::{Page, PaginationConfig},
    scope::Scope,
};

use super::{
    addressable::{Addressable, AddressableImpl},
    move_package::MovePackage,
    transaction::Transaction,
};

/// Interface implemented by versioned on-chain values that are addressable by an ID (also referred to as its address). This includes Move objects and packages.
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IObject",
    field(
        name = "version",
        ty = "UInt53",
        desc = "The version of this object that this content comes from.",
    ),
    field(
        name = "digest",
        ty = "String",
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
        ty = "Result<Option<Connection<CVersion, Object>>, RpcError<Error>>",
        desc = "Paginate all versions of this object after this one."
    ),
    field(
        name = "object_versions_before",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<CVersion>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<CVersion>"),
        arg(name = "filter", ty = "Option<VersionFilter>"),
        ty = "Result<Option<Connection<CVersion, Object>>, RpcError<Error>>",
        desc = "Paginate all versions of this object before this one."
    ),
    field(
        name = "previous_transaction",
        ty = "Result<Option<Transaction>, RpcError>",
        desc = "The transaction that created this version of the object"
    )
)]
pub(crate) enum IObject {
    MovePackage(MovePackage),
    Object(Object),
}

pub(crate) struct Object {
    pub(crate) super_: Addressable,
    pub(crate) version: SequenceNumber,
    pub(crate) digest: ObjectDigest,
    pub(crate) contents: Option<Arc<NativeObject>>,
}

/// Type to implement GraphQL fields that are shared by all Objects.
pub(crate) struct ObjectImpl<'o>(&'o Object);

/// Identifies a specific version of an object.
///
/// The `address` field must be specified, as well as at most one of `version`, `rootVersion`, or `atCheckpoint`. If none are provided, the object is fetched at the current checkpoint.
///
/// See `Query.object` for more details.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    /// The object's ID.
    pub(crate) address: SuiAddress,

    /// If specified, tries to fetch the object at this exact version.
    pub(crate) version: Option<UInt53>,

    /// If specified, tries to fetch the latest version of the object at or before this version.
    ///
    /// This can be used to fetch a child or ancestor object bounded by its root object's version. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    pub(crate) root_version: Option<UInt53>,

    /// If specified, tries to fetch the latest version as of this checkpoint.
    pub(crate) at_checkpoint: Option<UInt53>,
}

/// Filter for paginating the history of an Object or MovePackage.
#[derive(InputObject, Default, Debug)]
pub(crate) struct VersionFilter {
    /// Filter to versions that are strictly newer than this one, defaults to fetching from the earliest version known to this RPC (this could be the initial version, or some later version if the initial version has been pruned).
    after_version: Option<UInt53>,

    /// Filter to versions that are strictly older than this one, defaults to fetching up to the latest version (inclusive).
    before_version: Option<UInt53>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("At most one of a version, a root version, or a checkpoint bound can be specified when fetching an object")]
    OneBound,
}

pub(crate) type CVersion = JsonCursor<u64>;

/// An Object on Sui is either a typed value (a Move Object) or a Package (modules containing functions and types).
///
/// Every object on Sui is identified by a unique address, and has a version number that increases with every modification. Objects also hold metadata detailing their current owner (who can sign for access to the object and whether that access can modify and/or delete the object), and the digest of the last transaction that modified the object.
#[Object]
impl Object {
    /// The Object's ID.
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(&self.super_).address()
    }

    /// The version of this object that this content comes from.
    async fn version(&self) -> UInt53 {
        ObjectImpl::from(self).version()
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    async fn digest(&self) -> String {
        ObjectImpl::from(self).digest()
    }

    /// Attempts to convert the object into a MovePackage.
    async fn as_move_package(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<MovePackage>, RpcError<Error>> {
        MovePackage::from_object(self, ctx).await
    }

    /// Fetch the object with the same ID, at a different version, root version bound, or checkpoint.
    ///
    /// If no additional bound is provided, the latest version of this object is fetched at the latest checkpoint.
    async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Self>, RpcError<Error>> {
        ObjectImpl::from(self)
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError<Error>> {
        ObjectImpl::from(self).object_bcs(ctx).await
    }

    /// Paginate all versions of this object after this one.
    async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Connection<CVersion, Object>, RpcError<Error>> {
        ObjectImpl::from(self)
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
    }

    /// Paginate all versions of this object before this one.
    async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Connection<CVersion, Object>, RpcError<Error>> {
        ObjectImpl::from(self)
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
    }

    /// The transaction that created this version of the object.
    async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError<Error>> {
        ObjectImpl::from(self).previous_transaction(ctx).await
    }
}

impl Object {
    /// Construct an object that is represented by just its identifier (its object reference). This
    /// does not check whether the object exists, so should not be used to "fetch" an object based
    /// on an address and/or version provided as user input.
    pub(crate) fn with_ref(
        addressable: Addressable,
        version: SequenceNumber,
        digest: ObjectDigest,
    ) -> Self {
        Self {
            super_: addressable,
            version,
            digest,
            contents: None,
        }
    }

    /// Fetch an object by its key. The key can either specify an exact version to fetch, an
    /// upperbound against a "root version", an upperbound against a checkpoint, or none of the
    /// above.
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
            Ok(Self::at_version(ctx, scope, key.address, v).await?)
        } else if let Some(v) = key.root_version {
            Ok(Self::version_bounded(ctx, scope, key.address, v).await?)
        } else if let Some(cp) = key.at_checkpoint {
            let scope = scope.with_checkpoint_viewed_at(cp.into());
            Ok(Self::checkpoint_bounded(ctx, scope, key.address, cp).await?)
        } else {
            let cp: UInt53 = scope.checkpoint_viewed_at().into();
            Ok(Self::checkpoint_bounded(ctx, scope, key.address, cp).await?)
        }
    }

    /// Fetch the latest version of the object at the given address less than or equal to
    /// `root_version`.
    pub(crate) async fn version_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        root_version: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
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

        Object::from_stored_version(scope, stored)
    }

    /// Fetch the latest version of the object at the given address as of the checkpoint with
    /// sequence number `at_checkpoint`.
    pub(crate) async fn checkpoint_bounded(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        at_checkpoint: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
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
    pub(crate) async fn at_version(
        ctx: &Context<'_>,
        scope: Scope,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        let contents = contents(ctx, address, version);
        let stored_version =
            pg_loader.load_one(VersionedObjectVersionKey(address.into(), version.into()));
        let (contents, stored_version) = join!(contents, stored_version);

        let Some(c) = contents? else {
            return Ok(None);
        };

        if stored_version
            .context("Failed to get object version")?
            .is_none_or(|s| s.cp_sequence_number as u64 > scope.checkpoint_viewed_at())
        {
            return Ok(None);
        }

        Ok(Some(Self::from_contents(scope, c)))
    }

    /// Construct a GraphQL representation of an `Object` from its native representation.
    pub(crate) fn from_contents(scope: Scope, contents: Arc<NativeObject>) -> Self {
        let addressable = Addressable::with_address(scope, contents.id().into());

        Self {
            super_: addressable,
            version: contents.version(),
            digest: contents.digest(),
            contents: Some(contents),
        }
    }

    /// Construct a GraphQL representation of an `Object` from versioning information. This
    /// representation does not pre-fetch object contents.
    pub(crate) fn from_stored_version(
        scope: Scope,
        stored: StoredObjVersion,
    ) -> Result<Option<Self>, RpcError<Error>> {
        // Lack of an object digest indicates that the object was deleted or wrapped at this
        // version.
        let Some(digest) = stored.object_digest else {
            return Ok(None);
        };

        // If the object's version is from a later checkpoint than is being viewed currently, then
        // discard this result.
        if stored.cp_sequence_number as u64 > scope.checkpoint_viewed_at() {
            return Ok(None);
        }

        let addressable = Addressable::with_address(
            scope,
            NativeSuiAddress::from_bytes(stored.object_id)
                .context("Failed to deserialize SuiAddress")?,
        );

        Ok(Some(Object::with_ref(
            addressable,
            SequenceNumber::from_u64(stored.object_version as u64),
            ObjectDigest::try_from(&digest[..]).context("Failed to deserialize Object Digest")?,
        )))
    }

    /// Paginate through versions of an object (identified by its address).
    pub(crate) async fn paginate_by_version(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CVersion>,
        address: NativeSuiAddress,
        filter: VersionFilter,
    ) -> Result<Connection<CVersion, Object>, RpcError<Error>> {
        use obj_versions::dsl as v;

        let mut conn = Connection::new(false, false);

        let pg_reader: &PgReader = ctx.data()?;
        let mut query = v::obj_versions
            .filter(v::cp_sequence_number.le(scope.checkpoint_viewed_at() as i64))
            .filter(v::object_id.eq(address.to_vec()))
            .limit(page.limit() as i64 + 2)
            .into_boxed();

        if let Some(after_version) = filter.after_version {
            query = query.filter(v::object_version.gt(i64::from(after_version)));
        }

        if let Some(before_version) = filter.before_version {
            query = query.filter(v::object_version.lt(i64::from(before_version)));
        }

        query = if page.is_from_front() {
            query
                .order_by(v::cp_sequence_number)
                .then_order_by(v::object_version)
        } else {
            query
                .order_by(v::cp_sequence_number.desc())
                .then_order_by(v::object_version.desc())
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
            if let Some(object) = Self::from_stored_version(scope.clone(), stored)? {
                conn.edges.push(Edge::new(cursor, object));
            }
        }

        Ok(conn)
    }

    /// Returns a copy of this object but with its contents pre-fetched.
    pub(crate) async fn inflated(&self, ctx: &Context<'_>) -> Result<Self, RpcError<Error>> {
        Ok(Self {
            super_: self.super_.clone(),
            version: self.version,
            digest: self.digest,
            contents: self.contents(ctx).await?,
        })
    }

    /// Return a copy of the object's contents, either cached in the object or fetched from the KV
    /// store.
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Arc<NativeObject>>, RpcError<Error>> {
        if self.contents.is_some() {
            Ok(self.contents.clone())
        } else {
            contents(ctx, self.super_.address.into(), self.version.into()).await
        }
    }
}

impl ObjectImpl<'_> {
    pub(crate) fn version(&self) -> UInt53 {
        self.0.version.into()
    }

    pub(crate) fn digest(&self) -> String {
        Base58::encode(self.0.digest.inner())
    }

    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<Error>> {
        let key = ObjectKey {
            address: self.0.super_.address.into(),
            version,
            root_version,
            at_checkpoint: checkpoint,
        };

        Object::by_key(ctx, self.0.super_.scope.clone(), key).await
    }

    pub(crate) async fn object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<Error>> {
        let Some(object) = self.0.contents(ctx).await? else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(object.as_ref()).context("Failed to serialize object")?;
        Ok(Some(Base64(bytes)))
    }

    pub(crate) async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Connection<CVersion, Object>, RpcError<Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IObject", "objectVersionsAfter");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Apply any filter that was supplied to the query, but add an additional version
        // lowerbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            after_version: Some(self.0.version.value().into()),
            ..VersionFilter::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        Object::paginate_by_version(
            ctx,
            self.0.super_.scope.clone(),
            page,
            self.0.super_.address,
            filter,
        )
        .await
    }

    pub(crate) async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Result<Connection<CVersion, Object>, RpcError<Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IObject", "objectVersionsBefore");
        let page = Page::from_params(limits, first, after, last, before)?;

        // Apply any filter that was supplied to the query, but add an additional version
        // upperbound constraint.
        let Some(filter) = filter.unwrap_or_default().intersect(VersionFilter {
            before_version: Some(self.0.version.value().into()),
            ..VersionFilter::default()
        }) else {
            return Ok(Connection::new(false, false));
        };

        Object::paginate_by_version(
            ctx,
            self.0.super_.scope.clone(),
            page,
            self.0.super_.address,
            filter,
        )
        .await
    }

    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError<Error>> {
        let Some(object) = self.0.contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(Transaction::with_id(
            self.0.super_.scope.clone(),
            object.as_ref().previous_transaction,
        )))
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

impl<'o> From<&'o Object> for ObjectImpl<'o> {
    fn from(value: &'o Object) -> Self {
        ObjectImpl(value)
    }
}

/// Lazily load the contents of the object from the store.
async fn contents(
    ctx: &Context<'_>,
    address: SuiAddress,
    version: UInt53,
) -> Result<Option<Arc<NativeObject>>, RpcError<Error>> {
    let kv_loader: &KvLoader = ctx.data()?;
    Ok(kv_loader
        .load_one_object(address.into(), version.into())
        .await
        .context("Failed to fetch object contents")?
        .map(Arc::new))
}
