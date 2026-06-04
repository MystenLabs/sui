// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use bytes::Bytes;
use futures::Stream;
use futures::StreamExt;
use prometheus::Registry;
use sui_rpc::proto::sui::rpc::v2alpha as grpc_alpha;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as V2alphaLedgerServiceClient;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;
use tonic::transport::Uri;
use tower::Layer;
use tracing::warn;

use crate::ledger_grpc_reader::LedgerGrpcArgs;
use crate::metrics::GrpcMetricsLayer;
use crate::metrics::GrpcMetricsService;

const DEFAULT_MAX_DECODING_MESSAGE_SIZE: usize = 32 * 1024 * 1024;

/// A reader backed by the gRPC LedgerService's v2alpha experimental query APIs.
#[derive(Clone)]
pub struct AlphaLedgerGrpcReader {
    client: V2alphaLedgerServiceClient<GrpcMetricsService<Channel>>,
    timeout: Option<Duration>,
}

/// A page drained from a stream consisting of the items in stream order, the cursors of the first
/// and latest frames the stream emitted, and why the stream stopped.
///
/// `start_cursor` is the cursor of the first frame seen.
///
/// `end_cursor` may be beyond the last item in the collected page.
///
/// `end_reason` is `None` only when the stream terminated before a `QueryEnd` was received.
#[derive(Debug, Clone)]
pub struct StreamPage<I> {
    pub items: Vec<I>,
    pub start_cursor: Option<Bytes>,
    pub end_cursor: Option<Bytes>,
    pub end_reason: Option<grpc_alpha::QueryEndReason>,
}

#[derive(Debug)]
enum FrameKind<I> {
    Item { item: I, cursor: Option<Bytes> },
    Watermark { cursor: Option<Bytes> },
    End { reason: i32 },
    Unknown,
}

impl AlphaLedgerGrpcReader {
    pub async fn new(
        uri: Uri,
        args: LedgerGrpcArgs,
        prefix: Option<&str>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let mut endpoint = Channel::builder(uri.clone());
        if let Some(timeout) = args.statement_timeout() {
            endpoint = endpoint.timeout(timeout);
        }

        if uri.scheme_str() == Some("https") {
            let tls_config = ClientTlsConfig::new().with_native_roots();
            endpoint = endpoint.tls_config(tls_config)?;
        }

        let channel = endpoint.connect_lazy();
        let layered =
            GrpcMetricsLayer::new(prefix.unwrap_or("ledger_grpc"), registry).layer(channel);

        let timeout = args.statement_timeout();
        let max_decoding_message_size = args
            .ledger_grpc_max_decoding_message_size
            .unwrap_or(DEFAULT_MAX_DECODING_MESSAGE_SIZE);
        let client = V2alphaLedgerServiceClient::new(layered.clone())
            .max_decoding_message_size(max_decoding_message_size);

        Ok(Self { client, timeout })
    }

    /// Consumes the stream returned from a `list_transactions` request until server timeout or
    /// other terminal condition is met. The caller is responsible for resuming the next page from
    /// the `end_cursor` if there are more results to yield after the current page.
    pub async fn list_transactions(
        &self,
        request: grpc_alpha::ListTransactionsRequest,
    ) -> anyhow::Result<StreamPage<grpc_alpha::TransactionItem>> {
        let mut client = self.client.clone();
        let stream = client
            .list_transactions(self.request(request))
            .await
            .map_err(|s| anyhow::anyhow!("ListTransactions stream open failed: {}", s.message()))?
            .into_inner();

        drain_list_stream("ListTransactions", stream).await
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

impl<I> StreamPage<I> {
    /// True while the server has not exhausted the requested range.
    pub fn has_more(&self) -> bool {
        !matches!(
            self.end_reason,
            Some(
                grpc_alpha::QueryEndReason::CheckpointBound
                    | grpc_alpha::QueryEndReason::CursorBound
                    | grpc_alpha::QueryEndReason::LedgerTip
            )
        )
    }

    /// The cursor to continue paginating, or `None` if the requested range has been exhausted and
    /// no further pagination is possible.
    ///
    /// Invariant: `has_more()` ⇒ `end_cursor.is_some()`. Enforced at the page boundary by
    /// `drain_list_stream` (returns `data_loss` on violation) and preserved by any caller that
    /// re-synthesizes a `StreamPage` from a previously-validated one's `end_reason` +
    /// `end_cursor` fields together.
    pub fn next_cursor(&self) -> Option<&Bytes> {
        self.has_more().then(|| {
            self.end_cursor
                .as_ref()
                .expect("invariant: has_more implies end_cursor is Some")
        })
    }

    /// Fold one frame into the page. `start_cursor` latches on the first cursor seen; `end_cursor`
    /// tracks the latest.
    ///
    /// Returns `true` when the frame is the terminal `QueryEnd`.
    fn apply(&mut self, frame: FrameKind<I>) -> bool {
        match frame {
            FrameKind::Item { item, cursor } => {
                if cursor.is_some() {
                    if self.start_cursor.is_none() {
                        self.start_cursor = cursor.clone();
                    }
                    self.end_cursor = cursor;
                }
                self.items.push(item);
            }
            FrameKind::Watermark { cursor } => {
                if cursor.is_some() {
                    if self.start_cursor.is_none() {
                        self.start_cursor = cursor.clone();
                    }
                    self.end_cursor = cursor;
                }
            }
            FrameKind::End { reason } => {
                // Fold an unknown reason into `Unspecified` so `None` remains unambiguous shorthand
                // for "no End frame received" (i.e. the deadline cut the stream short).
                self.end_reason = match grpc_alpha::QueryEndReason::try_from(reason) {
                    Ok(decoded) => Some(decoded),
                    Err(_) => {
                        warn!(
                            reason_int = reason,
                            "list stream: server sent unknown QueryEndReason",
                        );
                        Some(grpc_alpha::QueryEndReason::Unspecified)
                    }
                };
                return true;
            }
            FrameKind::Unknown => {
                warn!("list stream: server sent empty or unrecognized Frame");
            }
        }
        false
    }
}

impl<I> Default for StreamPage<I> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            start_cursor: None,
            end_cursor: None,
            end_reason: None,
        }
    }
}

impl From<grpc_alpha::ListTransactionsResponse> for FrameKind<grpc_alpha::TransactionItem> {
    fn from(response: grpc_alpha::ListTransactionsResponse) -> Self {
        use grpc_alpha::list_transactions_response::Response;

        let Some(response) = response.response else {
            return FrameKind::Unknown;
        };
        match response {
            Response::Item(item) => {
                let cursor = item.watermark.as_ref().and_then(|w| w.cursor.clone());
                FrameKind::Item { item, cursor }
            }
            Response::Watermark(watermark) => FrameKind::Watermark {
                cursor: watermark.cursor,
            },
            Response::End(end) => FrameKind::End { reason: end.reason },
            _ => FrameKind::Unknown,
        }
    }
}

async fn drain_list_stream<R, I, S>(
    rpc_name: &'static str,
    stream: S,
) -> anyhow::Result<StreamPage<I>>
where
    R: Into<FrameKind<I>>,
    S: Stream<Item = Result<R, tonic::Status>>,
{
    futures::pin_mut!(stream);
    let mut page = StreamPage::default();
    loop {
        match stream.next().await {
            Some(Ok(response)) => {
                // Process and break on receiving `QueryEnd`.
                if page.apply(response.into()) {
                    break;
                }
            }
            // We expect the server to yield an `End` frame before reaching this branch.
            None => break,
            // `DeadlineExceeded`: server-side `grpc-timeout` header fired.
            // `Cancelled`: client-side channel timeout fired (or upstream cancel).
            // Both are timeout-shaped — preserve partial work if any progress was made;
            // propagate as error only if zero progress, so the caller can reshape.
            Some(Err(status))
                if matches!(
                    status.code(),
                    tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
                ) =>
            {
                if page.items.is_empty() && page.end_cursor.is_none() {
                    return Err(anyhow::anyhow!(
                        "{rpc_name} stream {:?} with no progress: {}",
                        status.code(),
                        status.message()
                    ));
                }
                break;
            }
            Some(Err(status)) => {
                return Err(anyhow::anyhow!(
                    "{rpc_name} stream error: {}",
                    status.message()
                ));
            }
        }
    }

    // Pagination is considered unresumable if there is more server-side work, but no valid cursor
    // was yielded (either from a standalone `Watermark` or the last `Item`'s watermark.)
    if page.has_more() && page.end_cursor.is_none() {
        return Err(anyhow::anyhow!(
            "{rpc_name}: server reported more results but did not advance cursor — cannot resume"
        ));
    }

    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_response(cursor: &[u8]) -> grpc_alpha::ListTransactionsResponse {
        let mut watermark = grpc_alpha::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut item = grpc_alpha::TransactionItem::default();
        item.watermark = Some(watermark);
        let mut response = grpc_alpha::ListTransactionsResponse::default();
        response.response = Some(grpc_alpha::list_transactions_response::Response::Item(item));
        response
    }

    fn watermark_response(cursor: &[u8]) -> grpc_alpha::ListTransactionsResponse {
        let mut watermark = grpc_alpha::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut response = grpc_alpha::ListTransactionsResponse::default();
        response.response = Some(grpc_alpha::list_transactions_response::Response::Watermark(
            watermark,
        ));
        response
    }

    fn end_response(reason: grpc_alpha::QueryEndReason) -> grpc_alpha::ListTransactionsResponse {
        let mut end = grpc_alpha::QueryEnd::default();
        end.reason = reason as i32;
        let mut response = grpc_alpha::ListTransactionsResponse::default();
        response.response = Some(grpc_alpha::list_transactions_response::Response::End(end));
        response
    }

    /// Drain a mocked stream. The turbofish pins `I = TransactionItem` for `drain_list_stream`
    /// since the `R: Into<FrameKind<I>>` bound alone doesn't let Rust infer `I` (Rust doesn't
    /// search `Into` impls for a unique match, and the return type doesn't propagate backward
    /// to inner generic calls).
    async fn drain_iter(
        responses: Vec<Result<grpc_alpha::ListTransactionsResponse, tonic::Status>>,
    ) -> anyhow::Result<StreamPage<grpc_alpha::TransactionItem>> {
        drain_list_stream::<_, grpc_alpha::TransactionItem, _>(
            "ListTransactions",
            futures::stream::iter(responses),
        )
        .await
    }

    #[test]
    fn drains_items_tracking_latest_cursor_and_end_reason() {
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(item_response(b"c1").into());
        page.apply(watermark_response(b"c2").into());
        page.apply(item_response(b"c3").into());
        page.apply(end_response(grpc_alpha::QueryEndReason::ItemLimit).into());

        assert_eq!(page.items.len(), 2);
        // `start_cursor` latches on the first cursor-bearing frame (the item at `c1`) and stays
        // there. `end_cursor` advances on every cursor-bearing frame — item OR standalone watermark
        // — so it went `c1` → `c2` (the watermark between items) → `c3` (the last item).
        assert_eq!(page.start_cursor.as_deref(), Some(b"c1".as_ref()));
        assert_eq!(page.end_cursor.as_deref(), Some(b"c3".as_ref()));
        assert_eq!(page.end_reason, Some(grpc_alpha::QueryEndReason::ItemLimit));
    }

    #[test]
    fn standalone_watermark_advances_cursor_without_items() {
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(watermark_response(b"w1").into());
        page.apply(watermark_response(b"w2").into());
        page.apply(end_response(grpc_alpha::QueryEndReason::LedgerTip).into());

        assert!(page.items.is_empty());
        assert_eq!(page.start_cursor.as_deref(), Some(b"w1".as_ref()));
        assert_eq!(page.end_cursor.as_deref(), Some(b"w2".as_ref()));
        assert_eq!(page.end_reason, Some(grpc_alpha::QueryEndReason::LedgerTip));
    }

    #[test]
    fn apply_signals_terminal_only_on_end_frame() {
        // The bool returned by `apply` is the drain loop's stop signal: `true` means "stop
        // draining," `false` means "keep going." Only the terminal `End` frame should signal stop.
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        assert!(!page.apply(item_response(b"c1").into()));
        assert!(!page.apply(watermark_response(b"w1").into()));
        // Outer message with no oneof set → `FrameKind::Unknown` → also non-terminal.
        assert!(!page.apply(grpc_alpha::ListTransactionsResponse::default().into()));
        assert!(page.apply(end_response(grpc_alpha::QueryEndReason::LedgerTip).into()));
    }

    #[test]
    fn start_cursor_latches_on_first_frame_and_does_not_update() {
        // Multiple cursor-bearing frames: `start_cursor` should remain at the first one,
        // `end_cursor` should track the latest.
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(watermark_response(b"w1").into());
        page.apply(item_response(b"c2").into());
        page.apply(watermark_response(b"w3").into());
        page.apply(item_response(b"c4").into());

        assert_eq!(page.start_cursor.as_deref(), Some(b"w1".as_ref()));
        assert_eq!(page.end_cursor.as_deref(), Some(b"c4".as_ref()));
    }

    #[test]
    fn has_more_true_when_truncated_or_timed_out() {
        // ITEM_LIMIT and SCAN_LIMIT both signal "we stopped short, resume here".
        for reason in [
            grpc_alpha::QueryEndReason::ItemLimit,
            grpc_alpha::QueryEndReason::ScanLimit,
        ] {
            let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
            page.apply(end_response(reason).into());
            assert!(page.has_more(), "expected has_more for {reason:?}");
        }

        // `end_reason == None` covers both the deadline cut-short case (no terminal frame
        // received) and any unrecognized / future-added variant — defaulting to "may have more"
        // avoids silent truncation.
        let page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        assert!(page.has_more());
    }

    #[test]
    fn has_more_false_when_range_exhausted() {
        for reason in [
            grpc_alpha::QueryEndReason::CheckpointBound,
            grpc_alpha::QueryEndReason::CursorBound,
            grpc_alpha::QueryEndReason::LedgerTip,
        ] {
            let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
            page.apply(end_response(reason).into());
            assert!(!page.has_more(), "expected !has_more for {reason:?}");
        }
    }

    #[test]
    fn apply_end_with_unknown_reason_folds_to_unspecified() {
        let mut end = grpc_alpha::QueryEnd::default();
        end.reason = i32::MAX;
        let mut response = grpc_alpha::ListTransactionsResponse::default();
        response.response = Some(grpc_alpha::list_transactions_response::Response::End(end));

        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(response.into());
        assert_eq!(
            page.end_reason,
            Some(grpc_alpha::QueryEndReason::Unspecified)
        );
    }

    #[test]
    fn apply_unknown_frame_does_not_mutate_page() {
        // Outer message with no oneof set — classifies to `FrameKind::Unknown`. `apply` should
        // warn but leave items / cursors / end_reason untouched.
        let response = grpc_alpha::ListTransactionsResponse::default();

        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(response.into());
        assert!(page.items.is_empty());
        assert_eq!(page.start_cursor, None);
        assert_eq!(page.end_cursor, None);
        assert_eq!(page.end_reason, None);
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
        assert_eq!(page.end_cursor.as_deref(), Some(b"c2".as_ref()));
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
    async fn drain_returns_page_on_half_close_after_progress() {
        // Server emitted one item, then half-closed without an End frame. The page is still
        // valid and resumable from the item's watermark.
        let page = drain_iter(vec![Ok(item_response(b"c1"))])
            .await
            .expect("partial-progress half-close should succeed");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.end_cursor.as_deref(), Some(b"c1".as_ref()));
        assert_eq!(page.end_reason, None);
    }
}
