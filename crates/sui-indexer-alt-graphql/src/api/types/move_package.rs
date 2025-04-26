// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{Context, Object};
use sui_types::move_package::MovePackage as NativeMovePackage;

use crate::{
    api::scalars::{base64::Base64, sui_address::SuiAddress, uint53::UInt53},
    error::RpcError,
};

use super::{
    addressable::AddressableImpl,
    object::{self, Object, ObjectImpl},
    transaction::Transaction,
};

pub(crate) struct MovePackage {
    /// Representation of this Move Package as a generic Object.
    super_: Object,

    /// Move package specific data, extracted from the native representation of the generic object.
    contents: NativeMovePackage,
}

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

    /// The Base64-encoded BCS serialization of this package, as an `Object`.
    pub(crate) async fn object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        ObjectImpl::from(&self.super_).object_bcs(ctx).await
    }

    /// The Base64-encoded BCS serialization of this package, as a `MovePackage`.
    async fn package_bcs(&self) -> Result<Option<Base64>, RpcError> {
        let bytes = bcs::to_bytes(&self.contents).context("Failed to serialize MovePackage")?;
        Ok(Some(Base64(bytes)))
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
}
