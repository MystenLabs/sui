// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{connection::Connection, Context, Interface, Object};
use sui_types::object::MoveObject as NativeMoveObject;
use tokio::sync::OnceCell;

use crate::{
    api::scalars::{base64::Base64, sui_address::SuiAddress, uint53::UInt53},
    error::RpcError,
};

use super::{
    address::AddressableImpl,
    move_type::MoveType,
    move_value::MoveValue,
    object::{self, CLive, CVersion, Object, ObjectImpl, VersionFilter},
    object_filter::{ObjectFilter, Validator as OFValidator},
    transaction::Transaction,
};

#[derive(Clone)]
pub(crate) struct MoveObject {
    /// Representation of this Move Object as a generic Object.
    super_: Object,

    /// Move object specific data, lazily loaded from the super object.
    native: OnceCell<Option<Arc<NativeMoveObject>>>,
}

/// Interface implemented by types that represent a Move object on-chain (A Move value whose type has `key`).
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IMoveObject",
    field(
        name = "contents",
        ty = "Result<Option<MoveValue>, RpcError<object::Error>>",
        desc = "The structured representation of the object's contents."
    ),
    field(
        name = "move_object_bcs",
        ty = "Result<Option<Base64>, RpcError<object::Error>>",
        desc = "The Base64-encoded BCS serialize of this object, as a `MoveObject`."
    )
)]
pub(crate) enum IMoveObject {
    MoveObject(MoveObject),
}

/// Type to implement GraphQL fields that are shared by all MoveObjects.
pub(crate) struct MoveObjectImpl<'o>(pub &'o MoveObject);

/// A MoveObject is a kind of Object that reprsents data stored on-chain.
#[Object]
impl MoveObject {
    /// The MoveObject's ID.
    pub(crate) async fn address(&self) -> SuiAddress {
        AddressableImpl::from(&self.super_.super_).address()
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self) -> UInt53 {
        ObjectImpl::from(&self.super_).version()
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self) -> String {
        ObjectImpl::from(&self.super_).digest()
    }

    /// The structured representation of the object's contents.
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<MoveValue>, RpcError<object::Error>> {
        MoveObjectImpl(self).contents(ctx).await
    }

    /// The Base64-encoded BCS serialize of this object, as a `MoveObject`.
    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        MoveObjectImpl(self).move_object_bcs(ctx).await
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
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_).object_bcs(ctx).await
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_)
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
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
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        AddressableImpl::from(&self.super_.super_)
            .objects(ctx, first, after, last, before, filter)
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

impl MoveObject {
    /// Create a `MoveObject` from an `Object` that is assumed to be a `MoveObject`. Its contents
    /// will be lazily loaded when needed, erroring if the `Object` is not a `MoveObject`.
    pub(crate) fn from_super(super_: Object) -> Self {
        Self {
            super_,
            native: OnceCell::new(),
        }
    }

    /// Try to upcast an `Object` to a `MoveObject`. This function returns `None` if `object`'s
    /// contents cannot be fetched, or it is not a move object.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let Some(super_contents) = object.contents(ctx).await? else {
            return Ok(None);
        };

        let Some(native) = super_contents.data.try_as_move() else {
            return Ok(None);
        };

        Ok(Some(Self {
            super_: object.clone(),
            native: OnceCell::from(Some(Arc::new(native.clone()))),
        }))
    }

    /// Get the native MoveObject, loading it lazily if needed.
    async fn native(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<Arc<NativeMoveObject>>, RpcError<object::Error>> {
        self.native
            .get_or_try_init(async || {
                let Some(contents) = self.super_.contents(ctx).await? else {
                    return Ok(None);
                };

                let native = contents
                    .data
                    .try_as_move()
                    .context("Object is not a MoveObject")?;

                Ok(Some(Arc::new(native.clone())))
            })
            .await
    }
}

impl MoveObjectImpl<'_> {
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<MoveValue>, RpcError<object::Error>> {
        let Some(native) = self.0.native(ctx).await? else {
            return Ok(None);
        };

        let type_ = MoveType::from_native(
            native.type_().clone().into(),
            self.0.super_.super_.scope.clone(),
        );

        Ok(Some(MoveValue::new(type_, native.contents().to_owned())))
    }

    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        let Some(native) = self.0.native(ctx).await? else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(native.as_ref()).context("Failed to serialize MoveObject")?;
        Ok(Some(Base64(bytes)))
    }
}
