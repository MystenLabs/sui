// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use anyhow::ensure;
use bytes::Bytes;
use futures::Stream;
use futures::StreamExt;
use prometheus::Registry;
use sui_rpc::Client;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use tonic::transport::Uri;
use tracing::warn;

use crate::ledger_grpc_reader::LedgerGrpcArgs;
use crate::metrics::GrpcMetricsLayer;

/// A reader backed by the gRPC LedgerService's streaming list APIs.
#[derive(Clone)]
pub struct AlphaLedgerGrpcReader {
    client: Client,
    timeout: Option<Duration>,
}

/// A single item from a list stream and the resume cursor the server emitted alongside it.
#[derive(Debug, Clone)]
pub struct PageItem<T> {
    pub payload: T,
    pub cursor: Bytes,
}

/// A page drained from a single gRPC list stream.
#[derive(Debug, Clone)]
pub struct StreamPage<T> {
    /// Items that matched the filters, in stream order.
    pub items: Vec<PageItem<T>>,
    first_wm_cursor: Option<Bytes>,
    last_wm_cursor: Option<Bytes>,
    pub end_reason: Option<proto::QueryEndReason>,
}

#[derive(Debug)]
enum FrameKind<T> {
    Frame {
        payload: Option<T>,
        cursor: Option<Bytes>,
        end_reason: Option<proto::QueryEndReason>,
    },
    /// A frame with none of the known fields set (unknown/future frame kind).
    Unknown,
}

impl AlphaLedgerGrpcReader {
    pub async fn new(
        uri: Uri,
        args: LedgerGrpcArgs,
        prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let timeout = args.statement_timeout();
        let mut client = Client::new(uri)?
            .with_max_decoding_message_size(args.ledger_grpc_max_decoding_message_size)
            .request_layer(GrpcMetricsLayer::new(
                prefix.unwrap_or("ledger_grpc"),
                registry,
            ));

        if let Some(timeout) = timeout {
            client = client.with_response_headers_timeout(timeout);
        }

        Ok(Self { client, timeout })
    }

    pub async fn list_transactions(
        &self,
        request: proto::ListTransactionsRequest,
    ) -> anyhow::Result<StreamPage<ExecutedTransaction>> {
        let stream = self
            .client
            .clone()
            .ledger_client()
            .list_transactions(self.request(request))
            .await
            .context("ListTransactions stream open failed")?
            .into_inner();

        drain_list_stream("ListTransactions", stream).await
    }

    pub async fn list_events(
        &self,
        request: proto::ListEventsRequest,
    ) -> anyhow::Result<StreamPage<proto::Event>> {
        let stream = self
            .client
            .clone()
            .ledger_client()
            .list_events(self.request(request))
            .await
            .context("ListEvents stream open failed")?
            .into_inner();

        drain_list_stream("ListEvents", stream).await
    }

    pub async fn list_checkpoints(
        &self,
        request: proto::ListCheckpointsRequest,
    ) -> anyhow::Result<StreamPage<proto::Checkpoint>> {
        let mut client = self.client.clone();
        let stream = client
            .list_checkpoints(self.request(request))
            .await
            .context("ListCheckpoints stream open failed")?
            .into_inner();

        drain_list_stream("ListCheckpoints", stream).await
    }

    /// Point-read an epoch. Returns `None` when the epoch does not exist.
    pub async fn get_epoch(
        &self,
        request: proto::GetEpochRequest,
    ) -> anyhow::Result<Option<proto::Epoch>> {
        let mut client = self.client.clone();
        let response = match client.get_epoch(self.request(request)).await {
            Ok(response) => response.into_inner(),
            Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
            Err(status) => {
                return Err(status).context("GetEpoch failed");
            }
        };

        Ok(response.epoch.filter(|e| *e != proto::Epoch::default()))
    }

    /// Create a gRPC request, optionally with the grpc-timeout header if configured.
    fn request<T>(&self, input: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(input);
        if let Some(timeout) = self.timeout {
            request.set_timeout(timeout);
        }
        request
    }
}

impl<T> StreamPage<T> {
    /// Whether further data may exist in the direction of pagination.
    ///
    /// `false` iff one of:
    /// - `reason ∈ {LedgerTip, CheckpointBound}` — authoritative range terminals.
    /// - `reason = CursorBound` AND no cursor was emitted - the server did not do any scanning and
    ///   short-circuited. Typically implies that the cursors for the request fell outside the
    ///   available range.
    pub fn has_more(&self) -> bool {
        use proto::QueryEndReason as R;
        match self.end_reason {
            None => true,
            Some(R::Unknown | R::ItemLimit | R::ScanLimit) => true,
            Some(R::LedgerTip | R::CheckpointBound) => false,
            Some(R::CursorBound) => self.last_cursor().is_some(),
            // `QueryEndReason` is non exhaustive — conservatively `true` if a
            // future variant slips past `apply()`'s `unwrap_or(Unknown)`.
            Some(_) => true,
        }
    }

    /// The page's starting cursor: the standalone-watermark cursor if one preceded any items,
    /// otherwise the first item's own cursor.
    pub fn first_cursor(&self) -> Option<&Bytes> {
        self.first_wm_cursor
            .as_ref()
            .or_else(|| self.items.first().map(|item| &item.cursor))
    }

    /// The page's resume cursor: a standalone-watermark cursor emitted after the last item if one
    /// exists, otherwise the last item's own cursor.
    pub fn last_cursor(&self) -> Option<&Bytes> {
        self.last_wm_cursor
            .as_ref()
            .or_else(|| self.items.last().map(|item| &item.cursor))
    }

    /// Construct a page directly for cross-crate tests, bypassing the drain loop. The watermark
    /// fields are private (their invariant is maintained by [`Self::apply`]); this is the only
    /// sanctioned way to set them from outside the crate.
    #[cfg(feature = "testing")]
    pub fn for_test(
        items: Vec<PageItem<T>>,
        first_wm_cursor: Option<Bytes>,
        last_wm_cursor: Option<Bytes>,
        end_reason: Option<proto::QueryEndReason>,
    ) -> Self {
        Self {
            items,
            first_wm_cursor,
            last_wm_cursor,
            end_reason,
        }
    }

    /// Fold one frame into the page.
    ///
    /// Returns `true` when the frame is `QueryEnd`.
    fn apply(&mut self, frame: FrameKind<T>) -> bool {
        let FrameKind::Frame {
            payload,
            cursor,
            end_reason,
        } = frame
        else {
            warn!("ignoring unrecognized frame");
            return false;
        };
        match payload {
            Some(payload) => {
                let cursor = cursor.expect("TryFrom validated item cursor");
                self.last_wm_cursor = None;
                self.items.push(PageItem { payload, cursor });
            }
            None => {
                if let Some(cursor) = cursor {
                    self.last_wm_cursor = Some(cursor.clone());
                    if self.items.is_empty() && self.first_wm_cursor.is_none() {
                        self.first_wm_cursor = Some(cursor);
                    }
                }
            }
        }
        if let Some(reason) = end_reason {
            // `QueryEnd::reason()` folds an absent or unknown reason into
            // `Unknown`, so `None` here remains unambiguous shorthand for
            // "no End frame received" (i.e. the deadline cut the stream short).
            self.end_reason = Some(reason);
            return true;
        }
        false
    }
}

impl<T> Default for StreamPage<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            first_wm_cursor: None,
            last_wm_cursor: None,
            end_reason: None,
        }
    }
}

impl TryFrom<proto::ListTransactionsResponse> for FrameKind<ExecutedTransaction> {
    type Error = anyhow::Error;

    fn try_from(response: proto::ListTransactionsResponse) -> anyhow::Result<Self> {
        classify_frame(response.transaction, response.watermark, response.end)
    }
}

impl TryFrom<proto::ListEventsResponse> for FrameKind<proto::Event> {
    type Error = anyhow::Error;

    fn try_from(response: proto::ListEventsResponse) -> anyhow::Result<Self> {
        classify_frame(response.event, response.watermark, response.end)
    }
}

impl TryFrom<proto::ListCheckpointsResponse> for FrameKind<proto::Checkpoint> {
    type Error = anyhow::Error;

    fn try_from(response: proto::ListCheckpointsResponse) -> anyhow::Result<Self> {
        classify_frame(response.checkpoint, response.watermark, response.end)
    }
}

/// Classify a raw list-stream response into a [`FrameKind`], given its payload field. Per-API
/// implementations only select which response field is the payload.
fn classify_frame<T>(
    payload: Option<T>,
    watermark: Option<proto::Watermark>,
    end: Option<proto::QueryEnd>,
) -> anyhow::Result<FrameKind<T>> {
    let cursor = watermark.and_then(|w| w.cursor);
    let end_reason = end.map(|e| e.reason());

    if payload.is_none() && cursor.is_none() && end_reason.is_none() {
        return Ok(FrameKind::Unknown);
    }
    if payload.is_some() && cursor.is_none() {
        bail!("Item frame missing watermark.cursor");
    }

    Ok(FrameKind::Frame {
        payload,
        cursor,
        end_reason,
    })
}

async fn drain_list_stream<R, T, S>(
    rpc_name: &'static str,
    stream: S,
) -> anyhow::Result<StreamPage<T>>
where
    R: TryInto<FrameKind<T>, Error = anyhow::Error>,
    S: Stream<Item = Result<R, tonic::Status>>,
{
    futures::pin_mut!(stream);
    let mut page = StreamPage::default();
    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                let frame = response
                    .try_into()
                    .with_context(|| format!("{rpc_name}: malformed frame"))?;
                // Process and break on receiving `QueryEnd`.
                if page.apply(frame) {
                    break;
                }
            }
            // `DeadlineExceeded`: server-side `grpc-timeout` header fired. `Cancelled`: client-side
            // channel timeout fired (or upstream cancel). In either case, preserve partial work if
            // any progress was made.
            Err(status)
                if matches!(
                    status.code(),
                    tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
                ) =>
            {
                break;
            }
            // Consider other errors as the request failed, safest to discard partial work.
            Err(status) => {
                bail!(
                    "{rpc_name}: stream error {:?}: {}",
                    status.code(),
                    status.message()
                );
            }
        }
    }

    // Exited via `break` or via `None`. If `has_more()` promises further data, the resume cursor
    // comes from the latest watermark received — on the fused last item, a ScanLimit end frame, or
    // a prior beacon; a bare end frame is only sent when no progress claim exists.
    ensure!(
        !page.has_more() || page.last_cursor().is_some(),
        "{rpc_name}: server reported more results but did not provide resume cursor — cannot continue",
    );

    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Well-formed frame with both `transaction` payload and a cursor-bearing watermark.
    fn item_response(cursor: &[u8]) -> proto::ListTransactionsResponse {
        let mut watermark = proto::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut response = proto::ListTransactionsResponse::default();
        response.transaction = Some(ExecutedTransaction::default());
        response.watermark = Some(watermark);
        response
    }

    fn watermark_response(cursor: &[u8]) -> proto::ListTransactionsResponse {
        let mut watermark = proto::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut response = proto::ListTransactionsResponse::default();
        response.watermark = Some(watermark);
        response
    }

    fn end_response(reason: proto::QueryEndReason) -> proto::ListTransactionsResponse {
        let mut end = proto::QueryEnd::default();
        end.reason = Some(reason as i32);
        let mut response = proto::ListTransactionsResponse::default();
        response.end = Some(end);
        response
    }

    fn end_response_with_cursor(
        cursor: &[u8],
        reason: proto::QueryEndReason,
    ) -> proto::ListTransactionsResponse {
        let mut response = end_response(reason);
        let mut watermark = proto::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        response.watermark = Some(watermark);
        response
    }

    fn frame(r: proto::ListTransactionsResponse) -> FrameKind<ExecutedTransaction> {
        r.try_into().expect("test fixture should be well-formed")
    }

    async fn drain_iter(
        responses: Vec<Result<proto::ListTransactionsResponse, tonic::Status>>,
    ) -> anyhow::Result<StreamPage<ExecutedTransaction>> {
        drain_list_stream::<_, ExecutedTransaction, _>(
            "ListTransactions",
            futures::stream::iter(responses),
        )
        .await
    }

    #[test]
    fn drains_items_tracking_latest_cursor_and_end_reason() {
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(item_response(b"c1")));
        page.apply(frame(watermark_response(b"w2")));
        let mut last = item_response(b"c3");
        last.end = end_response(proto::QueryEndReason::ItemLimit).end;
        page.apply(frame(last));
        assert_eq!(page.items.len(), 2);
        // Per-item cursors are preserved on `PageItem` — that's the whole point of the
        // payload/cursor split. The standalone watermark at `w2` does not produce a `PageItem`.
        assert_eq!(page.items[0].cursor.as_ref(), b"c1".as_ref());
        assert_eq!(page.items[1].cursor.as_ref(), b"c3".as_ref());
        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"c1".as_ref())
        );
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c3".as_ref()));
        assert_eq!(page.end_reason, Some(proto::QueryEndReason::ItemLimit));
    }

    #[test]
    fn standalone_watermark_advances_cursor_without_items() {
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(watermark_response(b"w1")));
        page.apply(frame(end_response_with_cursor(
            b"w2",
            proto::QueryEndReason::LedgerTip,
        )));

        assert!(page.items.is_empty());
        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"w1".as_ref())
        );
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"w2".as_ref()));
        assert_eq!(page.end_reason, Some(proto::QueryEndReason::LedgerTip));
    }

    #[test]
    fn apply_signals_stop_only_on_end_frame() {
        // The bool returned by `apply` is the drain loop's stop signal: `true` means "stop
        // draining," `false` means "keep going." Only frames carrying `end` should signal stop.
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        assert!(!page.apply(frame(item_response(b"c1"))));
        assert!(!page.apply(frame(watermark_response(b"w1"))));
        // Outer message with none of the known fields set → `FrameKind::Unknown` → continue.
        assert!(!page.apply(frame(proto::ListTransactionsResponse::default())));
        assert!(page.apply(frame(end_response(proto::QueryEndReason::LedgerTip))));

        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        let mut item_end = item_response(b"c2");
        item_end.end = end_response(proto::QueryEndReason::ItemLimit).end;
        assert!(page.apply(frame(item_end)));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].cursor.as_ref(), b"c2".as_ref());
        assert_eq!(page.end_reason, Some(proto::QueryEndReason::ItemLimit));
    }

    #[test]
    fn first_wm_cursor_set_to_first_pre_item_watermark() {
        // `first_wm_cursor` is set when watermark frame observed before items.
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(watermark_response(b"w1")));
        page.apply(frame(item_response(b"c2")));
        page.apply(frame(watermark_response(b"w3")));
        page.apply(frame(item_response(b"c4")));

        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"w1".as_ref())
        );
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c4".as_ref()));
    }

    #[test]
    fn first_wm_cursor_not_set_after_items() {
        // `first_wm_cursor` is never set once at least one item exists on the page.
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(item_response(b"c2")));
        page.apply(frame(watermark_response(b"w3")));
        page.apply(frame(item_response(b"c4")));
        page.apply(frame(watermark_response(b"w1")));

        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"c2".as_ref())
        );
        assert!(page.first_wm_cursor.is_none());
    }

    #[test]
    fn trailing_watermark_advances_past_last_item() {
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(item_response(b"c1")));
        page.apply(frame(watermark_response(b"w2")));

        // The trailing watermark's cursor wins over the item's — it represents
        // server progress past the last delivered item.
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"w2".as_ref()));
        // first_cursor falls back to the item, since no watermark preceded it.
        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"c1".as_ref())
        );
    }

    #[test]
    fn has_more_true_when_truncated_or_timed_out() {
        // ITEM_LIMIT and SCAN_LIMIT both signal "we stopped short, resume here".
        for reason in [
            proto::QueryEndReason::ItemLimit,
            proto::QueryEndReason::ScanLimit,
        ] {
            let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
            page.apply(frame(end_response(reason)));
            assert!(page.has_more(), "expected has_more for {reason:?}");
        }

        // `end_reason == None` covers both the deadline cut-short case (no end frame
        // received) and any unrecognized / future-added variant — defaulting to "may have more"
        // avoids silent truncation.
        let page: StreamPage<ExecutedTransaction> = StreamPage::default();
        assert!(page.has_more());
    }

    #[test]
    fn has_more_false_on_authoritative_terminals() {
        // `LedgerTip` and `CheckpointBound` are unconditional terminals — no data past tip /
        // outside the client's cp scope. `CursorBound` with no tracked cursor is the
        // short-circuit case (range collapsed at request resolution).
        for reason in [
            proto::QueryEndReason::CheckpointBound,
            proto::QueryEndReason::LedgerTip,
            proto::QueryEndReason::CursorBound,
        ] {
            let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
            page.apply(frame(end_response(reason)));
            assert!(!page.has_more(), "expected !has_more for {reason:?}");
        }
    }

    #[test]
    fn has_more_true_on_cursor_bound_with_tracked_cursor() {
        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        let mut response = item_response(b"c1");
        response.end = end_response(proto::QueryEndReason::CursorBound).end;
        page.apply(frame(response));

        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c1".as_ref()));
        assert!(
            page.has_more(),
            "CursorBound with tracked cursor should not be terminal"
        );
    }

    #[test]
    fn apply_end_with_unknown_reason_folds_to_unknown() {
        let mut end = proto::QueryEnd::default();
        end.reason = Some(i32::MAX);
        let mut response = proto::ListTransactionsResponse::default();
        response.end = Some(end);

        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(response));
        assert_eq!(page.end_reason, Some(proto::QueryEndReason::Unknown));
    }

    #[test]
    fn apply_unknown_frame_does_not_mutate_page() {
        // Outer message with none of the known fields set classifies to `FrameKind::Unknown`.
        // `apply` should warn but leave items / cursors / end_reason untouched.
        let response = proto::ListTransactionsResponse::default();

        let mut page: StreamPage<ExecutedTransaction> = StreamPage::default();
        page.apply(frame(response));
        assert!(page.items.is_empty());
        assert_eq!(page.first_cursor(), None);
        assert_eq!(page.last_cursor(), None);
        assert_eq!(page.end_reason, None);
    }

    #[test]
    fn try_from_item_without_cursor_errors() {
        // A frame with a `transaction` payload but no watermark cursor violates the
        // resumability contract — the conversion must fail loudly rather than
        // accepting an item that cannot be resumed from.
        let mut response = proto::ListTransactionsResponse::default();
        response.transaction = Some(ExecutedTransaction::default());

        let result: anyhow::Result<FrameKind<ExecutedTransaction>> = response.try_into();
        let err = result.expect_err("missing item cursor should error");
        assert!(
            err.to_string()
                .contains("Item frame missing watermark.cursor"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn drain_preserves_partial_progress_on_timeout() {
        let page = drain_iter(vec![
            Ok(item_response(b"c1")),
            Ok(item_response(b"c2")),
            Err(tonic::Status::cancelled("client channel timeout")),
        ])
        .await
        .expect("partial progress should be preserved");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c2".as_ref()));
        assert_eq!(page.end_reason, None);
        assert!(page.has_more());
    }

    #[tokio::test]
    async fn drain_errors_on_zero_progress_timeout() {
        drain_iter(vec![Err(tonic::Status::deadline_exceeded("server budget"))])
            .await
            .expect_err("zero-progress timeout should error");
    }

    #[tokio::test]
    async fn drain_errors_on_zero_progress_half_close() {
        drain_iter(vec![])
            .await
            .expect_err("zero-progress half-close should error");
    }

    #[tokio::test]
    async fn drain_propagates_non_timeout_status() {
        // A real upstream failure (not a timeout) — propagate as an error rather than
        // pretending the partial page is usable. Even with one item already collected, the
        // catch-all `Some(Err(status))` arm errors out.
        drain_iter(vec![
            Ok(item_response(b"c1")),
            Err(tonic::Status::internal("upstream blew up")),
        ])
        .await
        .expect_err("non-timeout status should propagate as error");
    }

    #[tokio::test]
    async fn drain_errors_on_malformed_item_frame() {
        // A malformed payload frame (no watermark cursor) reaches `drain_list_stream` via
        // `try_into`, which propagates the error out — partial work is discarded because the
        // cursor state is no longer trustworthy.
        let mut malformed = proto::ListTransactionsResponse::default();
        malformed.transaction = Some(ExecutedTransaction::default());

        drain_iter(vec![Ok(item_response(b"c0")), Ok(malformed)])
            .await
            .expect_err("malformed Item frame should error the drain");
    }

    #[tokio::test]
    async fn drain_returns_page_on_half_close_after_progress() {
        // Server emitted one item, then half-closed without an End frame. The page is still
        // valid and resumable from the item's watermark.
        let page = drain_iter(vec![Ok(item_response(b"c1"))])
            .await
            .expect("partial-progress half-close should succeed");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c1".as_ref()));
        assert_eq!(page.end_reason, None);
    }
}
