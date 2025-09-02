// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::{connection::Connection, Context, Interface, Object};
use futures::future::try_join_all;
use sui_types::{dynamic_field::DynamicFieldType, object::MoveObject as NativeMoveObject};
use tokio::sync::OnceCell;

use crate::{
    api::scalars::{
        base64::Base64, big_int::BigInt, sui_address::SuiAddress, type_filter::TypeInput,
        uint53::UInt53,
    },
    error::RpcError,
    pagination::{Page, PaginationConfig},
};

use super::{
    balance::{self, Balance},
    coin_metadata::CoinMetadata,
    dynamic_field::{DynamicField, DynamicFieldName},
    move_type::MoveType,
    move_value::MoveValue,
    object::{self, CLive, CVersion, Object, VersionFilter},
    object_filter::{ObjectFilter, Validator as OFValidator},
    owner::Owner,
    transaction::Transaction,
};

#[derive(Clone)]
pub(crate) struct MoveObject {
    /// Representation of this Move Object as a generic Object.
    pub(crate) super_: Object,

    /// Move object specific data, lazily loaded from the super object.
    native: Arc<OnceCell<Option<NativeMoveObject>>>,
}

/// Interface implemented by types that represent a Move object on-chain (A Move value whose type has `key`).
#[allow(clippy::duplicated_attributes)]
#[derive(Interface)]
#[graphql(
    name = "IMoveObject",
    field(
        name = "contents",
        ty = "Result<Option<MoveValue>, RpcError>",
        desc = "The structured representation of the object's contents."
    ),
    field(
        name = "dynamic_field",
        arg(name = "name", ty = "DynamicFieldName"),
        ty = "Result<Option<DynamicField>, RpcError<object::Error>>",
        desc = "Access a dynamic field on an object using its type and BCS-encoded name.",
    ),
    field(
        name = "dynamic_object_field",
        arg(name = "name", ty = "DynamicFieldName"),
        ty = "Result<Option<DynamicField>, RpcError<object::Error>>",
        desc = "Access a dynamic object field on an object using its type and BCS-encoded name.",
    ),
    field(
        name = "multi_get_dynamic_fields",
        arg(name = "keys", ty = "Vec<DynamicFieldName>"),
        ty = "Result<Vec<Option<DynamicField>>, RpcError<object::Error>>",
        desc = "Access dynamic fields on an object using their types and BCS-encoded names.\n\nReturns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.",
    ),
    field(
        name = "multi_get_dynamic_object_fields",
        arg(name = "keys", ty = "Vec<DynamicFieldName>"),
        ty = "Result<Vec<Option<DynamicField>>, RpcError<object::Error>>",
        desc = "Access dynamic object fields on an object using their types and BCS-encoded names.\n\nReturns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.",
    ),
    field(
        name = "dynamic_fields",
        arg(name = "first", ty = "Option<u64>"),
        arg(name = "after", ty = "Option<object::CLive>"),
        arg(name = "last", ty = "Option<u64>"),
        arg(name = "before", ty = "Option<object::CLive>"),
        ty = "Result<Option<Connection<String, DynamicField>>, RpcError<object::Error>>",
        desc = "Dynamic fields and dynamic object fields owned by this object.\n\nDynamic fields on wrapped objects can be accessed using `Address.dynamicFields`."
    ),
    field(
        name = "move_object_bcs",
        ty = "Result<Option<Base64>, RpcError<object::Error>>",
        desc = "The Base64-encoded BCS serialize of this object, as a `MoveObject`."
    )
)]
pub(crate) enum IMoveObject {
    CoinMetadata(CoinMetadata),
    DynamicField(DynamicField),
    MoveObject(MoveObject),
}

/// A MoveObject is a kind of Object that reprsents data stored on-chain.
#[Object]
impl MoveObject {
    /// The MoveObject's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Result<Option<UInt53>, RpcError> {
        self.super_.version(ctx).await
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Result<Option<String>, RpcError> {
        self.super_.digest(ctx).await
    }

    /// Attempts to convert the object into a CoinMetadata.
    pub(crate) async fn as_coin_metadata(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<CoinMetadata>, RpcError> {
        CoinMetadata::from_move_object(self, ctx).await
    }

    /// Attempts to convert the object into a DynamicField.
    pub(crate) async fn as_dynamic_field(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<DynamicField>, RpcError> {
        DynamicField::from_move_object(self, ctx).await
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

    /// The structured representation of the object's contents.
    pub(crate) async fn contents(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let type_ = MoveType::from_native(
            native.type_().clone().into(),
            self.super_.super_.scope.clone(),
        );

        Ok(Some(MoveValue::new(type_, native.contents().to_owned())))
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<String>, RpcError> {
        self.super_.default_suins_name(ctx).await
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError> {
        let scope = &self.super_.super_.scope;
        DynamicField::by_name(
            ctx,
            scope.clone(),
            self.super_.super_.address.into(),
            DynamicFieldType::DynamicField,
            name,
        )
        .await
    }

    /// Dynamic fields owned by this object.
    ///
    /// Dynamic fields on wrapped objects can be accessed using `Address.dynamicFields`.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
    ) -> Result<Option<Connection<String, DynamicField>>, RpcError<object::Error>> {
        let pagination: &PaginationConfig = ctx.data()?;
        let limits = pagination.limits("IMoveObject", "dynamicFields");
        let page = Page::from_params(limits, first, after, last, before)?;

        let dynamic_fields = DynamicField::paginate(
            ctx,
            self.super_.super_.scope.clone(),
            self.super_.super_.address.into(),
            page,
        )
        .await?;

        Ok(Some(dynamic_fields))
    }

    /// Access a dynamic object field on an object using its type and BCS-encoded name.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError> {
        let scope = &self.super_.super_.scope;
        DynamicField::by_name(
            ctx,
            scope.clone(),
            self.super_.super_.address.into(),
            DynamicFieldType::DynamicObject,
            name,
        )
        .await
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError> {
        let scope = &self.super_.super_.scope;
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                scope.clone(),
                self.super_.super_.address.into(),
                DynamicFieldType::DynamicField,
                key,
            )
        }))
        .await
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

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError> {
        let scope = &self.super_.super_.scope;
        try_join_all(keys.into_iter().map(|key| {
            DynamicField::by_name(
                ctx,
                scope.clone(),
                self.super_.super_.address.into(),
                DynamicFieldType::DynamicObject,
                key,
            )
        }))
        .await
    }

    /// The Base64-encoded BCS serialize of this object, as a `MoveObject`.
    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError> {
        let Some(native) = self.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let bytes = bcs::to_bytes(native).context("Failed to serialize MoveObject")?;
        Ok(Some(Base64(bytes)))
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
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Result<Option<Base64>, RpcError> {
        self.super_.object_bcs(ctx).await
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
        self.super_
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
    ) -> Result<Option<Connection<String, Object>>, RpcError> {
        self.super_
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
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Result<Option<Owner>, RpcError> {
        self.super_.owner(ctx).await
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError> {
        self.super_.previous_transaction(ctx).await
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<BigInt>, RpcError> {
        self.super_.storage_rebate(ctx).await
    }
}

impl MoveObject {
    /// Create a `MoveObject` from an `Object` that is assumed to be a `MoveObject`. Its contents
    /// will be lazily loaded when needed, erroring if the `Object` is not a `MoveObject`.
    pub(crate) fn from_super(super_: Object) -> Self {
        Self {
            super_,
            native: Arc::new(OnceCell::new()),
        }
    }

    /// Try to upcast an `Object` to a `MoveObject`. This function returns `None` if `object`'s
    /// contents cannot be fetched, or it is not a move object.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(super_contents) = object.contents(ctx).await?.as_ref() else {
            return Ok(None);
        };

        let Some(native) = super_contents.data.try_as_move() else {
            return Ok(None);
        };

        Ok(Some(Self {
            super_: object.clone(),
            native: Arc::new(OnceCell::from(Some(native.clone()))),
        }))
    }

    /// Get the native MoveObject, loading it lazily if needed.
    pub(crate) async fn native(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<NativeMoveObject>, RpcError> {
        self.native
            .get_or_try_init(async || {
                let Some(contents) = self.super_.contents(ctx).await?.as_ref() else {
                    return Ok(None);
                };

                let native = contents
                    .data
                    .try_as_move()
                    .context("Object is not a MoveObject")?;

                Ok(Some(native.clone()))
            })
            .await
    }
}
