// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{Context, InputObject, Object};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_types::{
    base_types::{SequenceNumber, SuiAddress as NativeSuiAddress},
    digests::ObjectDigest,
    object::Object as NativeObject,
};

use crate::{
    api::scalars::{base64::Base64, sui_address::SuiAddress, uint53::UInt53},
    error::RpcError,
};

use super::transaction::Transaction;

pub(crate) struct Object {
    address: NativeSuiAddress,
    version: SequenceNumber,
    digest: ObjectDigest,
    contents: ObjectContents,
}

/// The lazily loaded contents of an object.
#[derive(Clone)]
pub(crate) struct ObjectContents(Option<Arc<NativeObject>>);

/// Identifies a specific version of an object.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ObjectKey {
    pub address: SuiAddress,
    pub version: UInt53,
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
        self.version.into()
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    async fn digest(&self) -> String {
        Base58::encode(self.digest.inner())
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<ObjectContents, RpcError> {
        Ok(if self.contents.0.is_some() {
            self.contents.clone()
        } else {
            ObjectContents::fetch(ctx, self.address.into(), self.version.into()).await?
        })
    }
}

#[Object]
impl ObjectContents {
    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    async fn object_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let Some(object) = &self.0 else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(object.as_ref()).context("Failed to serialize object")?;
        Ok(Some(Base64(bytes)))
    }

    /// The transaction that created this version of the object.
    async fn previous_transaction(&self) -> Result<Option<Transaction>, RpcError> {
        let Some(object) = &self.0 else {
            return Ok(None);
        };

        Ok(Some(Transaction::with_id(object.previous_transaction)))
    }
}

impl Object {
    /// Construct an object that is represented by just its identifier (its object reference). This
    /// does not check whether the object exists, so should not be used to "fetch" an object based
    /// on an address and/or version provided as user input.
    #[allow(dead_code)] // TODO: Remove once this is used in object changes
    pub(crate) fn with_ref(
        address: NativeSuiAddress,
        version: SequenceNumber,
        digest: ObjectDigest,
    ) -> Self {
        Self {
            address,
            version,
            digest,
            contents: ObjectContents(None),
        }
    }

    /// Load the object at the given ID and version from the store, and return it fully inflated
    /// (with contents already fetched). Returns `None` if the object does not exist (either never
    /// existed or was pruned from the store).
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Option<Self>, RpcError> {
        let contents = ObjectContents::fetch(ctx, address, version).await?;
        let Some(object) = &contents.0 else {
            return Ok(None);
        };

        Ok(Some(Object {
            address: object.id().into(),
            version: object.version(),
            digest: object.digest(),
            contents,
        }))
    }
}

impl ObjectContents {
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        address: SuiAddress,
        version: UInt53,
    ) -> Result<Self, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;

        let object = kv_loader
            .load_one_object(address.into(), version.into())
            .await
            .context("Failed to fetch object contents")?;

        Ok(Self(object.map(Arc::new)))
    }
}
