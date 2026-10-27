// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Paging through a ledger List stream (`ListTransactions`, `ListEvents`) from GraphQL cursors.
//!
//! A cursor carrying ledger bytes is sent to the ledger service as is. A cursor carrying only a place
//! GraphQL knows narrows the request's checkpoint range to that place's checkpoint, and items on the
//! wrong side of it are dropped as they arrive. The page that comes back has GraphQL cursors minted
//! for its items and boundaries.

use std::ops::RangeInclusive;

use bytes::Bytes;
use sui_indexer_alt_reader::ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;

use crate::api::types::ledger_cursor::LedgerKey;
use crate::api::types::ledger_cursor::Point;
use crate::error::RpcError;
use crate::pagination::Page;

/// A cursor type that pages through a ledger List stream.
pub(crate) trait ScanCursor: Clone {
    type Key: LedgerKey;

    /// How this cursor bounds a page, as its `after` or its `before` cursor.
    fn bound(&self) -> RequestBound<Self::Key>;

    /// A cursor for ledger bytes that GraphQL places at `point`.
    fn from_ledger(bytes: Bytes, point: Option<Point<Self::Key>>) -> Self;

    /// A cursor at `point`, with no ledger bytes.
    fn from_point(point: Point<Self::Key>) -> Self;
}

/// One side of a List request, read from a GraphQL cursor.
#[derive(Clone)]
pub(crate) struct RequestBound<K> {
    /// Ledger bytes to send as the request's cursor.
    pub(crate) ledger: Option<Bytes>,

    /// Where GraphQL places the cursor. Items on the wrong side of it are dropped.
    pub(crate) point: Option<Point<K>>,
}

/// What one page asks a ledger List stream for.
pub(crate) struct ScanRequest {
    /// The page's ledger cursors, size, and ordering.
    pub(crate) options: v2::QueryOptions,
    /// Inclusive.
    pub(crate) start_checkpoint: u64,
    /// Exclusive.
    pub(crate) end_checkpoint: u64,
}

/// How a page's cursors and checkpoint range map onto a List request, and how its response maps
/// back onto GraphQL cursors.
pub(crate) struct ScanPlan<K> {
    ascending: bool,
    after: Option<RequestBound<K>>,
    before: Option<RequestBound<K>>,
    /// The page's checkpoint range, `[start, end)`, narrowed by cursors that carry no ledger bytes.
    start_checkpoint: u64,
    end_checkpoint: u64,
    /// Whether the range is narrowed on the far side, so that it ends at the far cursor's
    /// checkpoint rather than at the end of the data.
    far_narrowed: bool,
}

/// One page from a ledger List stream, with GraphQL cursors minted.
pub(crate) struct LedgerPage<T, C> {
    /// The page's items in scan order (descending when paginating from the back).
    pub(crate) items: Vec<LedgerItem<T, C>>,

    /// Where the page starts, in scan order.
    pub(crate) first_cursor: Option<C>,

    /// Where the next page in scan order resumes.
    pub(crate) last_cursor: Option<C>,

    /// The ledger service's latest coverage claim on the page: in scan order, every match at or
    /// before this checkpoint has been returned.
    pub(crate) covered_checkpoint: Option<u64>,

    pub(crate) end_reason: Option<QueryEndReason>,
}

pub(crate) struct LedgerItem<T, C> {
    pub(crate) payload: T,
    pub(crate) cursor: C,
    pub(crate) checkpoint: u64,
}

impl<K: LedgerKey> ScanPlan<K> {
    pub(crate) fn new<C>(page: &Page<C>, cp_bounds: RangeInclusive<u64>) -> Self
    where
        C: ScanCursor<Key = K>,
    {
        let ascending = page.is_from_front();
        let after = page.after().map(C::bound);
        let before = page.before().map(C::bound);

        let range_start = *cp_bounds.start();
        let range_end = cp_bounds.end().saturating_add(1);
        let start_checkpoint = point_only(after.as_ref())
            .map_or(range_start, |p| range_start.max(p.key().checkpoint()));
        let end_checkpoint = point_only(before.as_ref())
            .map_or(range_end, |p| range_end.min(end_checkpoint_before(p)));

        let far_narrowed = if ascending {
            end_checkpoint != range_end
        } else {
            start_checkpoint != range_start
        };

        Self {
            ascending,
            after,
            before,
            start_checkpoint,
            end_checkpoint,
            far_narrowed,
        }
    }

    /// The request for up to `limit` items, or `None` if the narrowed range is empty.
    pub(crate) fn request(&self, limit: usize) -> Option<ScanRequest> {
        if self.start_checkpoint >= self.end_checkpoint {
            return None;
        }

        let mut options = v2::QueryOptions::default();
        options.limit = Some(limit as u32);
        options.after = self.after.as_ref().and_then(|b| b.ledger.clone());
        options.before = self.before.as_ref().and_then(|b| b.ledger.clone());
        options.ordering = Some(if self.ascending {
            v2::Ordering::Ascending as i32
        } else {
            v2::Ordering::Descending as i32
        });

        Some(ScanRequest {
            options,
            start_checkpoint: self.start_checkpoint,
            end_checkpoint: self.end_checkpoint,
        })
    }

    /// Mint GraphQL cursors for the response to [`Self::request`], dropping items on the wrong
    /// side of a cursor's place. `key` reads an item's place from its payload.
    pub(crate) fn page<T, C>(
        &self,
        stream: StreamPage<T>,
        key: impl Fn(&T) -> anyhow::Result<K>,
    ) -> Result<LedgerPage<T, C>, RpcError>
    where
        C: ScanCursor<Key = K>,
    {
        let ascending = self.ascending;

        // The near cursor is the one the scan starts from, the far cursor the one it runs toward.
        let (near, far) = if ascending {
            (self.after.as_ref(), self.before.as_ref())
        } else {
            (self.before.as_ref(), self.after.as_ref())
        };
        let near_point = near.and_then(|b| b.point);
        let far_point = far.and_then(|b| b.point);

        let covered_checkpoint = stream.covered_checkpoint();
        let first_watermark = stream.first_watermark_cursor().cloned();
        let last_watermark = stream.last_watermark_cursor().cloned();
        let mut end_reason = stream.end_reason;

        // A range narrowed on the far side ends at the far cursor's checkpoint, not at the end of
        // the data, so reaching that end means reaching the far cursor.
        let mut reached_far =
            self.far_narrowed && end_reason == Some(QueryEndReason::CheckpointBound);

        let mut items = Vec::with_capacity(stream.items.len());

        // The last item scanned short of the far cursor, and whether it is on the page.
        let mut last_scanned: Option<(Bytes, K, bool)> = None;
        for item in stream.items {
            let key = key(&item.payload)?;
            if far_point.is_some_and(|p| !in_scan_before(&p, &key, ascending)) {
                reached_far = true;
                break;
            }

            let on_page = near_point.is_none_or(|p| in_scan_after(&p, &key, ascending));
            last_scanned = Some((item.cursor.clone(), key, on_page));
            if on_page {
                items.push(LedgerItem {
                    payload: item.payload,
                    cursor: C::from_ledger(item.cursor, Some(Point::At(key))),
                    checkpoint: key.checkpoint(),
                });
            }
        }

        if reached_far {
            end_reason = Some(QueryEndReason::CursorBound);
        }

        // The gap the page starts from in scan order: just past the near cursor, or the range's
        // edge.
        let near_gap = match near {
            Some(bound) => bound.point.map(|p| {
                if ascending {
                    p.gap_after()
                } else {
                    p.gap_before()
                }
            }),
            None if ascending => Some(Point::checkpoint_start(self.start_checkpoint)),
            None => Some(Point::checkpoint_start(self.end_checkpoint)),
        };

        let first_cursor = if near.is_some_and(|b| b.ledger.is_none()) {
            // The ledger never saw the near cursor, so no ledger bytes mark where the page starts.
            near_gap.map(C::from_point)
        } else if let Some(bytes) = first_watermark {
            Some(C::from_ledger(bytes, near_gap))
        } else {
            items.first().map(|item| item.cursor.clone())
        };

        let last_cursor = if reached_far {
            // The page ends at its last item, or at the far cursor itself when it has none.
            match items.last() {
                Some(item) => Some(item.cursor.clone()),
                None => far_point.map(|p| {
                    C::from_point(if ascending {
                        p.gap_before()
                    } else {
                        p.gap_after()
                    })
                }),
            }
        } else {
            let resume = || {
                resume_point(
                    ascending,
                    near_gap,
                    last_scanned.as_ref().map(|(_, key, _)| *key),
                    covered_checkpoint,
                )
            };

            match (last_watermark, &last_scanned) {
                (Some(bytes), _) => Some(C::from_ledger(bytes, resume())),
                (None, Some((_, _, true))) => items.last().map(|item| item.cursor.clone()),
                (None, Some((bytes, _, false))) => Some(C::from_ledger(bytes.clone(), resume())),
                (None, None) => None,
            }
        };

        Ok(LedgerPage {
            items,
            first_cursor,
            last_cursor,
            covered_checkpoint,
            end_reason,
        })
    }
}

impl<T, C> LedgerPage<T, C> {
    /// Whether more data may exist past the page, in scan order. Mirrors
    /// `StreamPage::has_more`: a page that stopped at its far cursor has more past it only if it
    /// has a cursor to resume from.
    pub(crate) fn has_more(&self) -> bool {
        use QueryEndReason as R;
        match self.end_reason {
            None => true,
            Some(R::Unknown | R::ItemLimit | R::ScanLimit) => true,
            Some(R::LedgerTip | R::CheckpointBound) => false,
            Some(R::CursorBound) => self.last_cursor.is_some(),
            Some(_) => true,
        }
    }

    /// A page over an empty range.
    fn empty() -> Self {
        Self {
            items: vec![],
            first_cursor: None,
            last_cursor: None,
            covered_checkpoint: None,
            end_reason: Some(QueryEndReason::CheckpointBound),
        }
    }
}

/// Request one page of `page` over `cp_bounds` through `list`, asking for up to `limit` items, and
/// mint cursors for what comes back. `key` reads an item's place from its payload.
pub(crate) async fn scan<C, T, F>(
    page: &Page<C>,
    cp_bounds: RangeInclusive<u64>,
    limit: usize,
    key: impl Fn(&T) -> anyhow::Result<C::Key>,
    list: impl FnOnce(ScanRequest) -> F,
) -> Result<LedgerPage<T, C>, RpcError>
where
    C: ScanCursor,
    F: Future<Output = anyhow::Result<StreamPage<T>>>,
{
    let plan = ScanPlan::new(page, cp_bounds);
    let Some(request) = plan.request(limit) else {
        return Ok(LedgerPage::empty());
    };

    let stream = list(request).await?;
    plan.page(stream, key)
}

/// The place a cursor names when it has no ledger bytes to send.
fn point_only<K: Copy>(bound: Option<&RequestBound<K>>) -> Option<Point<K>> {
    bound.filter(|b| b.ledger.is_none()).and_then(|b| b.point)
}

/// The exclusive end checkpoint of a range holding every item before `point`.
fn end_checkpoint_before<K: LedgerKey>(point: Point<K>) -> u64 {
    let checkpoint = point.key().checkpoint();
    if point.includes_before(&K::checkpoint_start(checkpoint)) {
        checkpoint.saturating_add(1)
    } else {
        checkpoint
    }
}

/// Whether the item at `key` comes after the near cursor at `point`, in scan order.
fn in_scan_after<K: LedgerKey>(point: &Point<K>, key: &K, ascending: bool) -> bool {
    if ascending {
        point.includes_after(key)
    } else {
        point.includes_before(key)
    }
}

/// Whether the item at `key` comes before the far cursor at `point`, in scan order.
fn in_scan_before<K: LedgerKey>(point: &Point<K>, key: &K, ascending: bool) -> bool {
    if ascending {
        point.includes_before(key)
    } else {
        point.includes_after(key)
    }
}

/// Where a page boundary at the end of the scan sits, from what bounds it: the gap the page started
/// from, the gap past the last item scanned, and the coverage claim. Each sits at or before the
/// boundary in scan order with no match in between, so the furthest of them has the same matches on
/// either side as the boundary. `None` if nothing bounds it.
fn resume_point<K: LedgerKey>(
    ascending: bool,
    near_gap: Option<Point<K>>,
    last_scanned: Option<K>,
    covered_checkpoint: Option<u64>,
) -> Option<Point<K>> {
    if ascending {
        let past_item = last_scanned.map(|key| Point::Before(key.successor()));
        let past_claim = covered_checkpoint.map(|c| Point::checkpoint_start(c.saturating_add(1)));
        [near_gap, past_item, past_claim]
            .into_iter()
            .flatten()
            .max()
    } else {
        let past_item = last_scanned.map(Point::Before);
        let past_claim = covered_checkpoint.map(Point::checkpoint_start);
        [near_gap, past_item, past_claim]
            .into_iter()
            .flatten()
            .min()
    }
}
