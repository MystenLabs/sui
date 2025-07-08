// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;

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

    // Read from kv store and retry if the object is not found.
    let mut object = None;
    let config = &ctx.config().objects;
    let mut interval = tokio::time::interval(Duration::from_millis(config.obj_retry_interval_ms));

    for _ in 0..=config.obj_retry_count {
        interval.tick().await;

        object = ctx
            .kv_loader()
            .load_one_object(object_id, latest_version.object_version as u64)
            .await
            .context("Failed to load latest object")?;
        if object.is_some() {
            break;
        }

        ctx.metrics()
            .read_retries
            .with_label_values(&["kv_object"])
            .inc();
    }

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
