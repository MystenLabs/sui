// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use sui_indexer_alt_reader::pg_reader::PgReader;

use crate::error::RpcError;
use crate::pagination::Page;

use crate::api::types::transaction::bloom::CpBoundsCursor;
use crate::api::types::transaction::bloom::candidate_cps;

/// Approximate false positive rate of the bloom filter. Used to inflate the initial
/// candidate fetch size before any real hit-rate data is available.
const BLOOM_FPR: f64 = 0.1;

/// Maximum number of bloom-matched candidate checkpoints to fetch in a single batch.
const MAX_LIMIT: usize = 500;

/// Scan state for paginating through bloom-filtered checkpoint candidates.
///
/// Tracks the current checkpoint window, hit rate, and iteration count to size
/// each batch of candidates returned by [`next`](Self::next).
///
/// ```ignore
/// while let Some(batch) = scan.next(&pg_reader, &filter_values).await? {
///     let hits = process(batch.candidates());
///     batch.record(hits);
/// }
/// ```
pub(super) struct BloomScan {
    cp_lo: u64,
    cp_hi_inclusive: u64,
    /// Target number of results to fill the page.
    page_limit: usize,
    /// Results that passed the real filter.
    hits: usize,
    iterations: usize,
    is_from_front: bool,
    /// The lowest non-zero per-batch hit rate observed so far, used as a pessimistic
    /// estimate for sizing subsequent fetches.
    min_batch_hit_rate: Option<f64>,
}

/// A set of bloom-matched candidate checkpoints returned by [`BloomScan::next`].
/// Holds a mutable borrow on the scan — call [`record`](Self::record) to feed back the
/// hit count and advance the scan window.
pub(super) struct Candidates<'a> {
    scan: &'a mut BloomScan,
    candidates: Vec<u64>,
}

impl BloomScan {
    pub(super) fn new<C: CpBoundsCursor>(page: &Page<C>, cp_bounds: &RangeInclusive<u64>) -> Self {
        let cp_lo = page.after().map_or(*cp_bounds.start(), |c| {
            c.cp_sequence_number().max(*cp_bounds.start())
        });
        let cp_hi_inclusive = page.before().map_or(*cp_bounds.end(), |c| {
            c.cp_sequence_number().min(*cp_bounds.end())
        });
        Self {
            cp_lo,
            cp_hi_inclusive,
            hits: 0,
            iterations: 0,
            page_limit: page.limit_with_overhead(),
            is_from_front: page.is_from_front(),
            min_batch_hit_rate: None,
        }
    }

    /// Fetches the next batch of bloom-matched checkpoints, sized based on the hit rate
    /// observed so far. Bloom filters operate on the values inserted and values inserted
    /// from busy packages may cause a high false positive rate for other items,
    /// so candidates include false positives — low hit rates cause larger fetches to compensate.
    ///
    /// Returns `None` when the page is filled or the checkpoint window is exhausted.
    pub(super) async fn next<'a>(
        &'a mut self,
        pg_reader: &PgReader,
        filter_values: &[Vec<u8>],
    ) -> Result<Option<Candidates<'a>>, RpcError> {
        if self.hits >= self.page_limit || self.cp_lo > self.cp_hi_inclusive {
            return Ok(None);
        }

        let limit = self.limit();
        let candidates = candidate_cps(
            pg_reader,
            filter_values,
            self.cp_lo,
            self.cp_hi_inclusive,
            self.is_from_front,
            limit,
        )
        .await?;

        Ok(if candidates.is_empty() {
            None
        } else {
            Some(Candidates {
                scan: self,
                candidates,
            })
        })
    }

    fn record(&mut self, candidates: &[u64], batch_hits: usize) {
        let batch_size = candidates.len();
        if batch_size == 0 {
            return;
        }

        if batch_hits > 0 {
            let batch_rate = batch_hits as f64 / batch_size as f64;
            self.min_batch_hit_rate = Some(
                self.min_batch_hit_rate
                    .map_or(batch_rate, |r| r.min(batch_rate)),
            );
        }

        self.iterations += 1;
        self.hits += batch_hits;

        let Some(&last_cp) = candidates.last() else {
            return;
        };

        if self.is_from_front {
            self.cp_lo = last_cp.saturating_add(1);
        } else {
            self.cp_hi_inclusive = last_cp.saturating_sub(1);
        }
    }

    /// Estimates how many bloom-matched candidate checkpoints to request based on the current precision.
    ///                                                                                                                                               
    /// Before any hits are observed, assumes the STARTING_HIT_RATE and                                                                            
    /// doubles the request size each round to handle unknown sparsity.               
    ///  
    /// Once hits are observed, switches to the empirical hit rate with
    /// an overfetch multiplier to overfetch and reduce the chance of needing another round.
    ///
    /// The result is always clamped to `[remaining, MAX_LIMIT]` — at least
    /// enough to fill the page at 100% hit rate, and at most MAX_LIMIT to
    /// bound per-round work.
    fn limit(&self) -> usize {
        let remaining = self.page_limit.saturating_sub(self.hits);

        let estimate = match self.min_batch_hit_rate {
            None => self.page_limit as f64 / (1.0 - BLOOM_FPR) * f64::exp2(self.iterations as f64),
            Some(rate) => remaining as f64 / rate,
        };

        estimate.clamp(remaining as f64, MAX_LIMIT as f64) as usize
    }
}

impl<'a> Candidates<'a> {
    pub(super) fn candidates(&self) -> &[u64] {
        &self.candidates
    }

    /// Records how many candidates matched the real filter and advances the scan window.
    pub(super) fn record(self, hits: usize) {
        self.scan.record(&self.candidates, hits);
    }
}
