// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::anyhow;
use async_graphql::Context;

use crate::config::Limits;
use crate::error::{RpcError, resource_exhausted};

/// Meters the number of "rich" queries performed in a single request. Rich queries are queries that
/// require dedicated requests to the backing store (i.e. they cannot be batched using a
/// data-loader).
#[derive(Default)]
pub(crate) struct Meter(AtomicUsize);

/// Increment the rich query meter by one. If the meter exceeds the configured limit, a
/// `RESOURCE_EXHAUSTED` error is returned.
pub(crate) fn debit<E>(ctx: &Context<'_>) -> Result<(), RpcError<E>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let limits: &Limits = ctx.data_unchecked();
    let meter: &Meter = ctx.data_unchecked();

    // Use fetch_add with Relaxed ordering since we only need atomicity, not synchronization.
    // The comparison against the limit will catch any overflow.
    let prev = meter.0.fetch_add(1, Ordering::Relaxed);
    if prev >= limits.max_rich_queries {
        return Err(resource_exhausted(anyhow!(
            "Exceeded the maximum number ({}) of queries that require dedicated access to a \
             backing store in a single request.",
            limits.max_rich_queries
        )));
    }
    Ok(())
}
