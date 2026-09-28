// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;
use sui_types::derived_object::derive_object_id;

use crate::api::types::dynamic_field;
use crate::api::types::dynamic_field::NameKey;
use crate::api::types::move_object::MoveObject;
use crate::api::types::object;
use crate::api::types::object::Object;
use crate::error::RpcError;
use crate::scope::Scope;

/// Look up a derived object using its parent, key, and optional version bound.
pub(crate) async fn by_key(
    ctx: &Context<'_>,
    scope: Scope,
    key: NameKey,
) -> Result<Option<MoveObject>, RpcError<dynamic_field::Error>> {
    let (type_, bcs) = key.name.eval(ctx).await?;
    let address = derive_object_id(key.parent, &type_, &bcs)?.into();

    let object = Object::by_key(
        ctx,
        scope.without_root_bound(),
        object::ObjectKey {
            address,
            version: key.version,
            root_version: key.root_version,
            at_checkpoint: key.at_checkpoint,
        },
    )
    .await
    .map_err(crate::error::convert)?;

    Ok(object.map(MoveObject::from_super))
}
