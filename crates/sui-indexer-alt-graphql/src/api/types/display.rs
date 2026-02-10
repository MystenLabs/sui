// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::SimpleObject;
use async_graphql::dataloader::DataLoader;
use move_core_types::language_storage::StructTag;
use sui_indexer_alt_reader::displays::DisplayKey;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::display_registry;

use crate::api::scalars::json::Json;
use crate::api::types::object::Object;
use crate::error::RpcError;
use crate::scope::Scope;

/// A rendered JSON blob based on an on-chain template.
#[derive(SimpleObject)]
pub(crate) struct Display {
    /// Output for all successfully substituted display fields. Unsuccessful fields will be `null`, and will be accompanied by a field in `errors`, explaining the error.
    pub(crate) output: Option<Json>,

    /// If any fields failed to render, this will contain a mapping from failed field names to error messages. If all fields succeed, this will be `null`.
    pub(crate) errors: Option<Json>,
}

/// Try to load the V1 Display format for this type.
pub(crate) async fn display_v1(
    ctx: &Context<'_>,
    type_: StructTag,
) -> Result<Option<DisplayVersionUpdatedEvent>, RpcError> {
    let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

    let Some(stored) = pg_loader
        .load_one(DisplayKey(type_))
        .await
        .context("Failed to fetch Display v1")?
    else {
        return Ok(None);
    };

    let event: DisplayVersionUpdatedEvent = bcs::from_bytes(&stored.display)
        .context("Failed to deserialize DisplayVersionUpdatedEvent")?;

    Ok(Some(event))
}

/// Try to load the V2 Display format for this type.
pub(crate) async fn display_v2(
    ctx: &Context<'_>,
    scope: Scope,
    type_: StructTag,
) -> Result<Option<display_registry::Display>, RpcError> {
    let object_id = display_registry::display_object_id(type_.into())
        .context("Failed to derive Display V2 object ID")?;

    let Some(object) = Object::latest(ctx, scope, object_id.into()).await? else {
        return Ok(None);
    };

    let Some(native) = object.contents(ctx).await? else {
        return Ok(None);
    };

    let Some(move_object) = native.data.try_as_move() else {
        return Ok(None);
    };

    let display: display_registry::Display = bcs::from_bytes(move_object.contents())
        .context("Failed to deserialize Display V2 object")?;

    Ok(Some(display))
}
