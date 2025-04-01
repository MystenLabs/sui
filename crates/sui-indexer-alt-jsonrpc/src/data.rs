// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use serde::de::DeserializeOwned;
use sui_indexer_alt_reader::object_versions::LatestObjectVersionKey;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::context::Context;

/// Load the contents of the live version of an object from the store and deserialize it as an
/// `Object`. Returns `None` if the object is deleted, wrapped, or never existed.
pub(crate) async fn load_live(
    ctx: &Context,
    object_id: ObjectID,
) -> Result<Option<Object>, anyhow::Error> {
    let Some(latest_version) = ctx
        .pg_loader()
        .load_one(LatestObjectVersionKey(object_id))
        .await
        .context("Failed to load latest version")?
    else {
        return Ok(None);
    };

    if latest_version.object_digest.is_none() {
        return Ok(None);
    }

    let object = ctx
        .kv_loader()
        .load_one_object(object_id, latest_version.object_version as u64)
        .await
        .context("Failed to load latest object")?;

    Ok(object)
}

/// Fetch the latest version of the object at ID `object_id`, and deserialize its contents as a
/// Rust type `T`, assuming that it exists and is a Move object (not a package).
pub(crate) async fn load_live_deserialized<T: DeserializeOwned>(
    ctx: &Context,
    object_id: ObjectID,
) -> Result<T, anyhow::Error> {
    let object = load_live(ctx, object_id).await?.context("No data found")?;

    let move_object = object.data.try_as_move().context("Not a Move object")?;
    bcs::from_bytes(move_object.contents()).context("Failed to deserialize Move value")
}
