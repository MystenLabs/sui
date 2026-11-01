// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Paging through a ledger List stream (`ListTransactions`, `ListEvents`) from GraphQL cursors.
//!
//! Every GraphQL cursor has an exact place in ledger order. A page's request starts and ends at its
//! cursors' places, narrowed to the page's checkpoint range, and carries each cursor's ledger bytes
//! (if it has any) in the matching slot: `after` with the start, `before` with the end. The page
//! that comes back has GraphQL cursors minted for its items, at their keys, and for its boundaries,
//! at the `cursor_position` the ledger service reports for them.

use std::ops::RangeInclusive;

use bytes::Bytes;
use sui_indexer_alt_reader::ledger_grpc_reader::PageBoundary;
use sui_indexer_alt_reader::ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;

use crate::api::types::ledger_cursor::LedgerCursor;
use crate::api::types::ledger_cursor::LedgerKey;
use crate::api::types::ledger_cursor::LedgerToken;
use crate::api::types::ledger_cursor::Point;
use crate::error::RpcError;
use crate::pagination::Page;

/// A List request's start or end fields, naming one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PositionFields {
    pub(crate) checkpoint: u64,
    pub(crate) transaction_index: Option<u64>,
    pub(crate) event_index: Option<u32>,
}

/// What one page asks a ledger List stream for.
pub(crate) struct ScanRequest {
    /// The page's ledger cursors, size, and ordering.
    pub(crate) options: v2::QueryOptions,
    /// The request's start: it holds the matches at or after this position.
    pub(crate) start: PositionFields,
    /// The request's end: it holds the matches before this position.
    pub(crate) end: PositionFields,
}

/// How a page's cursors and checkpoint range map onto a List request, and how its response maps
/// back onto GraphQL cursors.
pub(crate) struct ScanPlan<K> {
    ascending: bool,
    after_bytes: Option<Bytes>,
    before_bytes: Option<Bytes>,
    /// The page's span, `[start, end)`: its checkpoint range narrowed to its cursors' places.
    start: K,
    end: K,
    /// Whether the span's far edge, in scan order, is the far cursor's place rather than the edge
    /// of the checkpoint range.
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

impl PositionFields {
    /// The start fields of a request holding the matches at or after `position`.
    pub(crate) fn start<K: LedgerKey>(position: K) -> Self {
        let (transaction_index, event_index) = position.transaction_and_event_index();
        Self {
            checkpoint: position.checkpoint(),
            transaction_index: (transaction_index != 0 || event_index != 0)
                .then_some(transaction_index),
            event_index: (event_index != 0).then_some(event_index),
        }
    }

    /// The end fields of a request holding the matches before `position`. A position at a
    /// checkpoint's start ends the request before that checkpoint; any other position ends it
    /// within its own checkpoint.
    pub(crate) fn end<K: LedgerKey>(position: K) -> Self {
        let (transaction_index, event_index) = position.transaction_and_event_index();
        if transaction_index == 0 && event_index == 0 {
            Self {
                checkpoint: position.checkpoint(),
                transaction_index: None,
                event_index: None,
            }
        } else {
            Self {
                checkpoint: position.checkpoint().saturating_add(1),
                transaction_index: Some(transaction_index),
                event_index: (event_index != 0).then_some(event_index),
            }
        }
    }
}

impl<K: LedgerKey> ScanPlan<K> {
    pub(crate) fn new(page: &Page<LedgerCursor<K>>, cp_bounds: RangeInclusive<u64>) -> Self {
        let ascending = page.is_from_front();
        let after = page.after();
        let before = page.before();

        let range_start = K::checkpoint_start(*cp_bounds.start());
        let range_end = K::checkpoint_start(cp_bounds.end().saturating_add(1));
        let start = after.map_or(range_start, |c| {
            c.point().gap_after().key().max(range_start)
        });
        let end = before.map_or(range_end, |c| c.point().gap_before().key().min(range_end));

        let far_narrowed = if ascending {
            end < range_end
        } else {
            start > range_start
        };

        Self {
            ascending,
            after_bytes: after.and_then(|c| c.ledger_bytes()).cloned(),
            before_bytes: before.and_then(|c| c.ledger_bytes()).cloned(),
            start,
            end,
            far_narrowed,
        }
    }

    /// The request for up to `limit` items, or `None` if the page's span is empty.
    pub(crate) fn request(&self, limit: usize) -> Option<ScanRequest> {
        if self.start >= self.end {
            return None;
        }

        let mut options = v2::QueryOptions::default();
        options.limit = Some(limit as u32);
        options.after = self.after_bytes.clone();
        options.before = self.before_bytes.clone();
        options.ordering = Some(if self.ascending {
            v2::Ordering::Ascending as i32
        } else {
            v2::Ordering::Descending as i32
        });

        Some(ScanRequest {
            options,
            start: PositionFields::start(self.start),
            end: PositionFields::end(self.end),
        })
    }

    /// Mint GraphQL cursors for the response to [`Self::request`]. `key` reads an item's place from
    /// its payload.
    pub(crate) fn page<T>(
        &self,
        stream: StreamPage<T>,
        key: impl Fn(&T) -> anyhow::Result<K>,
    ) -> Result<LedgerPage<T, LedgerCursor<K>>, RpcError> {
        // The edge of the span the scan starts from, and the one it runs toward.
        let (near, far) = if self.ascending {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };

        let first_watermark = stream.first_watermark().map(boundary_cursor);
        let last_watermark = stream.last_watermark().map(boundary_cursor);
        let covered_checkpoint = stream.covered_checkpoint();

        // A request whose far edge is the far cursor's place ends there rather than at the end of
        // the data, so reaching that end means reaching the far cursor.
        let reached_far_place =
            self.far_narrowed && stream.end_reason == Some(QueryEndReason::CheckpointBound);
        let end_reason = if reached_far_place {
            Some(QueryEndReason::CursorBound)
        } else {
            stream.end_reason
        };

        let mut items = Vec::with_capacity(stream.items.len());
        for item in stream.items {
            let key = key(&item.payload)?;
            items.push(LedgerItem {
                payload: item.payload,
                cursor: LedgerToken::from_ledger(item.cursor, Point::At(key)),
                checkpoint: key.checkpoint(),
            });
        }

        // With no frame to start from, the page starts at the near edge of its span.
        let first_cursor = first_watermark
            .or_else(|| items.first().map(|item| item.cursor.clone()))
            .or_else(|| Some(LedgerToken::from_point(Point::Before(near))));

        // With no frame to resume from, a page that reached the far cursor's place resumes there.
        let last_cursor = last_watermark
            .or_else(|| items.last().map(|item| item.cursor.clone()))
            .or_else(|| reached_far_place.then(|| LedgerToken::from_point(Point::Before(far))));

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
}

/// Request one page of `page` over `cp_bounds` through `list`, asking for up to `limit` items, and
/// mint cursors for what comes back. `key` reads an item's place from its payload.
pub(crate) async fn scan<K, T, F>(
    page: &Page<LedgerCursor<K>>,
    cp_bounds: RangeInclusive<u64>,
    limit: usize,
    key: impl Fn(&T) -> anyhow::Result<K>,
    list: impl FnOnce(ScanRequest) -> F,
) -> Result<LedgerPage<T, LedgerCursor<K>>, RpcError>
where
    K: LedgerKey,
    F: Future<Output = anyhow::Result<StreamPage<T>>>,
{
    let plan = ScanPlan::new(page, cp_bounds);
    let Some(request) = plan.request(limit) else {
        // The cursors' places leave the span empty: the page ends at the span's edge, as the
        // ledger service ends a scan of an empty range, and reaches the far cursor when it narrowed
        // that edge.
        let mut stream = StreamPage::default();
        stream.end_reason = Some(QueryEndReason::CheckpointBound);
        return plan.page(stream, key);
    };

    let stream = list(request).await?;
    plan.page(stream, key)
}

/// The cursor of a frame without an item, at the position the ledger service reports for it.
fn boundary_cursor<K: LedgerKey>(boundary: &PageBoundary) -> LedgerCursor<K> {
    LedgerToken::from_ledger(
        boundary.cursor.clone(),
        Point::Before(K::from_position(&boundary.position)),
    )
}

#[cfg(test)]
mod tests {
    use crate::api::types::ledger_cursor::EventKey;
    use crate::api::types::ledger_cursor::TransactionKey;

    use super::*;

    fn tx(checkpoint: u64, transaction_index: u64) -> TransactionKey {
        TransactionKey {
            checkpoint,
            transaction_index,
        }
    }

    fn ev(checkpoint: u64, transaction_index: u64, event_index: u32) -> EventKey {
        EventKey {
            checkpoint,
            transaction_index,
            event_index,
        }
    }

    fn fields(
        checkpoint: u64,
        transaction_index: Option<u64>,
        event_index: Option<u32>,
    ) -> PositionFields {
        PositionFields {
            checkpoint,
            transaction_index,
            event_index,
        }
    }

    /// A start sends the indexes it needs: none at a checkpoint's start, and the transaction index
    /// whenever the event index is set.
    #[test]
    fn start_fields_from_positions() {
        assert_eq!(PositionFields::start(tx(5, 0)), fields(5, None, None));
        assert_eq!(PositionFields::start(tx(5, 2)), fields(5, Some(2), None));
        assert_eq!(PositionFields::start(ev(5, 0, 0)), fields(5, None, None));
        assert_eq!(PositionFields::start(ev(5, 2, 0)), fields(5, Some(2), None));
        assert_eq!(
            PositionFields::start(ev(5, 0, 1)),
            fields(5, Some(0), Some(1))
        );
        assert_eq!(
            PositionFields::start(ev(5, 2, 1)),
            fields(5, Some(2), Some(1))
        );
    }

    /// An end at a checkpoint's start ends before that checkpoint; any other end ends within its
    /// own checkpoint, the last one the request covers.
    #[test]
    fn end_fields_from_positions() {
        assert_eq!(PositionFields::end(tx(5, 0)), fields(5, None, None));
        assert_eq!(PositionFields::end(tx(5, 2)), fields(6, Some(2), None));
        assert_eq!(PositionFields::end(ev(5, 0, 0)), fields(5, None, None));
        assert_eq!(PositionFields::end(ev(5, 2, 0)), fields(6, Some(2), None));
        assert_eq!(
            PositionFields::end(ev(5, 0, 1)),
            fields(6, Some(0), Some(1))
        );
        assert_eq!(
            PositionFields::end(ev(5, 2, 1)),
            fields(6, Some(2), Some(1))
        );
    }
}
