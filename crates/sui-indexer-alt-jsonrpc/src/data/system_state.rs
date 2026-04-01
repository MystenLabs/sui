// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_indexer_alt_schema::schema::kv_feature_flags;
use sui_types::committee::EpochId;

use crate::context::Context;

/// Query the latest epoch from the database.
pub(crate) async fn latest_epoch(ctx: &Context) -> Result<EpochId, anyhow::Error> {
    use kv_epoch_starts::dsl as e;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let epoch: i64 = conn
        .first(e::kv_epoch_starts.select(e::epoch).order(e::epoch.desc()))
        .await
        .context("Failed to fetch the current epoch")?;

    Ok(epoch as EpochId)
}

/// Query the latest value for a given feature flag, from the database.
pub(crate) async fn latest_feature_flag(ctx: &Context, name: &str) -> Result<bool, anyhow::Error> {
    use kv_feature_flags::dsl as f;

    let mut conn = ctx
        .pg_reader()
        .connect()
        .await
        .context("Failed to connect to the database")?;

    let query = f::kv_feature_flags
        .select(f::flag_value)
        .filter(f::flag_name.eq(name))
        .order(f::protocol_version.desc())
        .limit(1);

    let flag: bool = conn
        .results(query)
        .await
        .with_context(|| format!("Failed to fetch latest flag value for '{name}'"))?
        .first()
        .copied()
        .unwrap_or(false);

    Ok(flag)
}
