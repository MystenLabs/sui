// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod address_balance_coins;
mod object;

use anyhow::Context as _;
use diesel::ExpressionMethods;
use diesel::QueryDsl;
use sui_indexer_alt_schema::schema::kv_epoch_starts;
use sui_types::committee::EpochId;

use crate::context::Context;

pub(crate) use address_balance_coins::load_address_balance_coin;
pub(crate) use address_balance_coins::try_resolve_address_balance_object;
pub(crate) use object::load_live;
pub(crate) use object::load_live_deserialized;

/// Query the latest epoch from the database.
pub(crate) async fn current_epoch(ctx: &Context) -> Result<EpochId, anyhow::Error> {
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
