// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{dataloader::DataLoader, Context, InputObject, Interface, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::{
    kv_loader::KvLoader,
    object_versions::{CheckpointBoundedObjectVersionKey, VersionBoundedObjectVersionKey},
    pg_reader::PgReader,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress as NativeSuiAddress},
    digests::ObjectDigest,
    object::Object as NativeObject,
};

use crate::{
    api::scalars::{base64::Base64, sui_address::SuiAddress, uint53::UInt53},
    error::{bad_user_input, RpcError},
};

use super::transaction::Transaction;

/// Interface implemented by versioned on-chain values that are addressable by an ID (also referred to as its address). This includes Move objects and packages.
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
        name = "object_bcs",
        ty = "Result<Option<Base64>, RpcError>",
        desc = "The Base64-encoded BCS serialization of this object, as an `Object`."
    ),
    field(
        name = "previous_transaction",
        ty = "Result<Option<Transaction>, RpcError>",
        desc = "The transaction that created this version of the object"
    )
)]
pub(crate) enum IObject {
    Object(Object),
}

pub(crate) struct Object {
    address: NativeSuiAddress,
    version: SequenceNumber,
    digest: ObjectDigest,
    contents: Option<Arc<NativeObject>>,
}

/// Type to implement GraphQL fields that are shared by all Objects.
pub(crate) struct ObjectImpl<'o>(&'o Object);

/// Identifies a specific version of an object.
///
/// The `address` field must be specified, as well as exactly one of `version` or `rootVersion`.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    /// The object's ID.
    pub address: SuiAddress,

    /// If specified, tries to fetch the object at this exact version.
    pub version: Option<UInt53>,

    /// If specified, tries to fetch the latest version of the object at or before this version.
    ///
    /// This can be used to fetch a child or ancestor object bounded by its root object's version. For any wrapped or child (object-owned) object, its root object can be defined recursively as:
    ///
    /// - The root object of the object it is wrapped in, if it is wrapped.
    /// - The root object of its owner, if it is owned by another object.
    /// - The object itself, if it is not object-owned or wrapped.
    pub root_version: Option<UInt53>,

    /// If specified, tries to fetch the latest version as of this checkpoint.
    pub at_checkpoint: Option<UInt53>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub(crate) enum Error {
    #[error("Operation not supported")]
    NotSupported,

    #[error("At most one of a version or a root version can be specified when fetching an object")]
    OneBound,
}

/// An Object on Sui is either a typed value (a Move Object) or a Package (modules containing functions and types).
///
/// Every object on Sui is identified by a unique address, and has a version number that increases with every modification. Objects also hold metadata detailing their current owner (who can sign for access to the object and whether that access can modify and/or delete the object), and the digest of the last transaction that modified the object.
#[Object]
impl Object {
    /// The Object's ID.
    async fn address(&self) -> SuiAddress {
        self.address.into()
    }

    /// The version of this object that this content comes from.
    async fn version(&self) -> UInt53 {
        ObjectImpl::from(self).version()
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    async fn digest(&self) -> String {
        ObjectImpl::from(self).digest()
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError<Error>> {
        ObjectImpl::from(self).object_bcs(ctx).await
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
        address: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    ) -> Self {
        Self {
            address: address.into(),
            version,
            digest,
            contents: None,
        }
    }

    /// Fetch an object by its key. The key can either specify an exact version to fetch, an
    /// upperbound against a "root version", or an upperbound against a checkpoint.
    pub(crate) async fn by_key(
        ctx: &Context<'_>,
        key: ObjectKey,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let bounds = key.version.is_some() as u8
            + key.root_version.is_some() as u8
            + key.at_checkpoint.is_some() as u8;

        if bounds > 1 {
            Err(bad_user_input(Error::OneBound))
        } else if let Some(v) = key.version {
            Ok(Self::at_version(ctx, key.address, v).await?)
        } else if let Some(v) = key.root_version {
            Ok(Self::version_bounded(ctx, key.address, v).await?)
        } else if let Some(cp) = key.at_checkpoint {
            Ok(Self::checkpoint_bounded(ctx, key.address, cp).await?)
        } else {
            Err(bad_user_input(Error::NotSupported))
        }
    }

    /// Fetch the latest version of the object at the given address less than or equal to
    /// `root_version`.
    pub(crate) async fn version_bounded(
        ctx: &Context<'_>,
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

        // Lack of an object digest indicates that the object was deleted or wrapped at this
        // version.
        let Some(digest) = stored.object_digest else {
            return Ok(None);
        };

        Ok(Some(Object::with_ref(
            ObjectID::from_bytes(stored.object_id).context("Failed to deserialize Object ID")?,
            SequenceNumber::from_u64(stored.object_version as u64),
            ObjectDigest::try_from(&digest[..]).context("Failed to deserialize Object Digest")?,
        )))
    }

    /// Fetch the latest version of the object at the given address as of the checkpoint with
    /// sequence number `at_checkpoint`.
    pub(crate) async fn checkpoint_bounded(
        ctx: &Context<'_>,
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

        // Lack of an object digest indicates that the object was deleted or wrapped at this
        // version.
        let Some(digest) = stored.object_digest else {
            return Ok(None);
        };

        Ok(Some(Object::with_ref(
            ObjectID::from_bytes(stored.object_id).context("Failed to deserialize Object ID")?,
            SequenceNumber::from_u64(stored.object_version as u64),
            ObjectDigest::try_from(&digest[..]).context("Failed to deserialize Object Digest")?,
        )))
    }

    /// Load the object at the given ID and version from the store, and return it fully inflated
    /// (with contents already fetched). Returns `None` if the object does not exist (either never
    /// existed or was pruned from the store).
    pub(crate) async fn at_version(
        ctx: &Context<'_>,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError<Error>> {
        let Some(object) = contents(ctx, address, version).await? else {
            return Ok(None);
        };

        Ok(Some(Object {
            address: object.id().into(),
            version: object.version(),
            digest: object.digest(),
            contents: Some(object),
        }))
    }

    /// Return a copy of the object's contents, either cached in the object or fetched from the KV
    /// store.
    async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Arc<NativeObject>>, RpcError<Error>> {
        if self.contents.is_some() {
            Ok(self.contents.clone())
        } else {
            contents(ctx, self.address.into(), self.version.into()).await
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

    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError<Error>> {
        let Some(object) = self.0.contents(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(Transaction::with_id(
            object.as_ref().previous_transaction,
        )))
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
