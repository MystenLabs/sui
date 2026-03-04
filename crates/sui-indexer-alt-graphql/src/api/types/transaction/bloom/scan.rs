// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use async_graphql::Context;

use crate::error::RpcError;
use crate::pagination::Page;

use super::CpBoundsCursor;

/// Safety margin multiplier applied to hit-rate estimates to account for variance
/// in the observed rate.
const OVERFETCH_MULTIPLIER: f64 = 5.0;

/// Approximate true positive rate of the bloom filter (1 - FPR). With a ~70% FPR,
/// ~30% of bloom-matched candidates actually contain the filter value.
const BLOOM_TRUE_POSITIVE_RATE: f64 = 0.30;

/// Maximum number of SQL+KV iterations before returning partial results. Prevents unbounded
/// scanning when filters match no or very few results (e.g. nonexistent event types).
/// Estimated to cap at ~5s assuming ~50ms per iteration at max page size.
const MAX_SCAN_ITERATIONS: usize = 100;

const MAX_LIMIT: usize = 500;

/// Iterates over checkpoint ranges using bloom filters to find candidates that may
/// match a query filter. Callers use `next` to fetch batches of candidate checkpoints
/// and `update` to feed back how many results passed the real filter, which the scan
/// uses to adaptively size the next batch.
pub(super) struct BloomScan {
    cp_lo: u64,
    cp_hi_inclusive: u64,
    /// Target number of results to fill the page.
    page_limit: usize,
    /// Bloom-matched checkpoints examined so far.
    candidates_seen: usize,
    /// Results that passed the real filter.
    hits: usize,
    rounds: usize,
    is_from_front: bool,
}

impl BloomScan {
    pub fn new<C: CpBoundsCursor>(page: &Page<C>, cp_bounds: &RangeInclusive<u64>) -> Self {
        let cp_lo = page.after().map_or(*cp_bounds.start(), |c| {
            c.cp_sequence_number().max(*cp_bounds.start())
        });
        let cp_hi_inclusive = page.before().map_or(*cp_bounds.end(), |c| {
            c.cp_sequence_number().min(*cp_bounds.end())
        });
        Self {
            cp_lo,
            cp_hi_inclusive,
            candidates_seen: 0,
            hits: 0,
            rounds: 0,
            page_limit: page.limit_with_overhead(),
            is_from_front: page.is_from_front(),
        }
    }

    /// Fetches the next batch of bloom-matched checkpoints, or `None` if
    /// the scan is complete (page filled, iteration limit reached, or window exhausted).
    pub async fn next<C>(
        &self,
        ctx: &Context<'_>,
        filter_values: &[[u8; 32]],
        page: &Page<C>,
    ) -> Result<Option<Vec<u64>>, RpcError> {
        if self.hits >= self.page_limit
            || self.rounds >= MAX_SCAN_ITERATIONS
            || self.cp_lo > self.cp_hi_inclusive
        {
            return Ok(None);
        }

        let limit = self.limit();
        let candidates = super::candidate_cps(
            ctx,
            filter_values,
            self.cp_lo,
            self.cp_hi_inclusive,
            page,
            limit,
        )
        .await?;

        Ok(if candidates.is_empty() {
            None
        } else {
            Some(candidates)
        })
    }

    /// Records the outcome of processing a batch of candidates for limit calculations and narrows the
    /// checkpoint range to exclude scanned candidates.
    pub fn update(&mut self, candidates: &[u64], hits: usize) {
        self.candidates_seen += candidates.len();
        self.rounds += 1;
        self.hits = hits;

        let Some(&last_cp) = candidates.last() else {
            return;
        };

        if self.is_from_front {
            self.cp_lo = last_cp.saturating_add(1);
        } else {
            self.cp_hi_inclusive = last_cp.saturating_sub(1);
        }
    }

    /// Estimates how many bloom-matched candidate checkpoints to request in the next
    /// scan based on the observed hit rate so far.
    ///
    /// When no hits have been observed, accounts for the bloom FPR and doubles the limit
    /// for the unknown real hit density:
    ///   Round 0: 52 / 0.30 = 173
    ///   Round 1: 52 / 0.30 * 2 = 347
    ///   Round 2: 52 / 0.30 * 4 = 693 → clamped to MAX_LIMIT (500)
    ///
    /// Once hits are observed, uses the end-to-end hit rate (bloom TPR * hit density)
    /// with a safety margin:
    ///   5 hits from 260 seen → hit_rate ≈ 0.02, remaining = 47
    ///   estimate = 47 / 0.02 * 5 = 11,750 → clamped to 500
    fn limit(&self) -> usize {
        let remaining = self.page_limit.saturating_sub(self.hits);

        let estimate = if self.hits == 0 {
            self.page_limit as f64 / BLOOM_TRUE_POSITIVE_RATE * f64::exp2(self.rounds as f64)
        } else {
            let hit_rate = self.hits as f64 / self.candidates_seen as f64;
            remaining as f64 / hit_rate * OVERFETCH_MULTIPLIER
        };

        estimate.clamp(remaining as f64, MAX_LIMIT as f64) as usize
    }
}
