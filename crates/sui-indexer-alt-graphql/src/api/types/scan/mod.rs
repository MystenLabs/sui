// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::iter::Rev;
use std::ops::Range;
use std::ops::RangeInclusive;

use async_graphql::Context;
use async_graphql::connection::Connection;
use itertools::Either;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::types::event::Event;
use crate::api::types::event::SCEvent;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::transaction::SCTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::pagination::Page;
use crate::scope::Scope;

mod bloom;
pub(crate) mod cursor;
mod lookups;
mod paginate;

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

    let digests_by_cp = lookups::load_digests(ctx, candidate_cps.clone())
        .await
        .map_err(upcast)?;

    let tx_digests = digests_by_cp.values().flatten().copied().collect();
    let transactions = lookups::load_transactions(ctx, tx_digests)
        .await
        .map_err(upcast)?;

    paginate::transaction_results(
        scope,
        filter,
        page,
        &candidate_cps,
        &digests_by_cp,
        &transactions,
    )
}

pub(crate) async fn events(
    ctx: &Context<'_>,
    scope: Scope,
    filter: &EventFilter,
    page: &Page<SCEvent>,
    cp_bounds: RangeInclusive<u64>,
    limits: &Limits,
) -> Result<Connection<String, Event>, RpcError<ScanError>> {
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

    let digests_by_cp = lookups::load_digests(ctx, candidate_cps.clone())
        .await
        .map_err(upcast)?;

    let tx_digests = digests_by_cp.values().flatten().copied().collect();
    let events = lookups::load_events(ctx, tx_digests)
        .await
        .map_err(upcast)?;

    paginate::event_results(scope, filter, page, &candidate_cps, &digests_by_cp, &events)
}

// Check overall scan limits and bounds with cursors applied are valid ranges.
pub(crate) fn validate_bounds<C: cursor::ScanCursor>(
    cp_bounds: RangeInclusive<u64>,
    page: &Page<JsonCursor<C>>,
    limits: &Limits,
) -> Result<Option<(u64, u64)>, RpcError<ScanError>> {
    let cp_lo = page.after().map_or(*cp_bounds.start(), |a| {
        (*cp_bounds.start()).max(a.cp_sequence_number())
    });
    let cp_hi = page.before().map_or(*cp_bounds.end(), |b| {
        (*cp_bounds.end()).min(b.cp_sequence_number())
    });

    if cp_lo > cp_hi {
        return Ok(None);
    }

    let scan_range = cp_hi.saturating_sub(cp_lo).saturating_add(1);
    if scan_range > limits.max_scan_limit {
        return Err(bad_user_input(ScanError::LimitExceeded {
            requested: scan_range,
            max: limits.max_scan_limit,
        }));
    }

    Ok(Some((cp_lo, cp_hi)))
}

/// A bidirectional iterator over a range based on page direction.
pub(crate) fn directional_iter<C>(
    page: &Page<C>,
    range: Range<usize>,
) -> Either<Range<usize>, Rev<Range<usize>>> {
    if page.is_from_front() {
        Either::Left(range)
    } else {
        Either::Right(range.rev())
    }
}
