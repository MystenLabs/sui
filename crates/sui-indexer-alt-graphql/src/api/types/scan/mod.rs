// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod bloom;
mod lookup;
mod paginate;

use std::ops::RangeInclusive;

use async_graphql::Context;
use async_graphql::connection::Connection;

use crate::api::types::transaction::SCTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::pagination::Page;
use crate::scope::Scope;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ScanError {
    #[error(
        "Scan range of {requested} checkpoints exceeds maximum of {max}. \
         Use afterCheckpoint/beforeCheckpoint filters to narrow the range."
    )]
    LimitExceeded { requested: u64, max: u64 },
}

pub(crate) async fn transactions(
    ctx: &Context<'_>,
    scope: Scope,
    filter: &TransactionFilter,
    page: &Page<SCTransaction>,
    cp_bounds: RangeInclusive<u64>,
    limits: &Limits,
) -> Result<Connection<String, Transaction>, RpcError<ScanError>> {
    let Some((cp_lo, cp_hi)) = validate_bounds(cp_bounds, page, limits)? else {
        return Ok(Connection::new(false, false));
    };

    let filter_values = filter.bloom_probe_values();
    let candidate_cps = bloom::candidate_cps(ctx, &filter_values, cp_lo, cp_hi, page)
        .await
        .map_err(upcast)?;

    if candidate_cps.is_empty() {
        return Ok(Connection::new(false, false));
    }

    let (digests, transactions) = lookup::load_transactions(ctx, &candidate_cps)
        .await
        .map_err(upcast)?;

    paginate::results(scope, filter, page, candidate_cps, digests, transactions)
}

fn validate_bounds(
    cp_bounds: RangeInclusive<u64>,
    page: &Page<SCTransaction>,
    limits: &Limits,
) -> Result<Option<(u64, u64)>, RpcError<ScanError>> {
    let cp_lo = page.after().map_or(*cp_bounds.start(), |a| {
        (*cp_bounds.start()).max(a.cp_sequence_number)
    });
    let cp_hi = page.before().map_or(*cp_bounds.end(), |b| {
        (*cp_bounds.end()).min(b.cp_sequence_number)
    });

    if cp_lo > cp_hi {
        return Ok(None);
    }

    let scan_range = cp_hi.saturating_sub(cp_lo);
    if scan_range > limits.max_scan_limit {
        return Err(bad_user_input(ScanError::LimitExceeded {
            requested: scan_range,
            max: limits.max_scan_limit,
        }));
    }

    Ok(Some((cp_lo, cp_hi)))
}
