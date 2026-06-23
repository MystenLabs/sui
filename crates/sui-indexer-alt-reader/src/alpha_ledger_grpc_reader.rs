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
use sui_rpc::proto::sui::rpc::v2alpha as grpc_alpha;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient;
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
    client: LedgerServiceClient<GrpcMetricsService<Channel>>,
    timeout: Option<Duration>,
}

/// A page drained from a single gRPC list stream.
#[derive(Debug, Clone)]
pub struct StreamPage<I> {
    /// Items that matched the filters, in stream order.
    pub items: Vec<I>,
    /// First cursor recorded from an item or watermark.
    pub first_cursor: Option<Bytes>,
    /// Last cursor recorded from an item or watermark.
    pub last_cursor: Option<Bytes>,
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
        let client = LedgerServiceClient::new(layered.clone())
            .max_decoding_message_size(max_decoding_message_size);

        Ok(Self { client, timeout })
    }

    pub async fn list_transactions(
        &self,
        request: grpc_alpha::ListTransactionsRequest,
    ) -> anyhow::Result<StreamPage<grpc_alpha::TransactionItem>> {
        let mut client = self.client.clone();
        let stream = client
            .list_transactions(self.request(request))
            .await
            .context("ListTransactions stream open failed")?
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
    /// Whether further data may exist in the direction of pagination.
    ///
    /// `false` iff one of:
    /// - `reason ∈ {LedgerTip, CheckpointBound}` — authoritative range terminals.
    /// - `reason = CursorBound` AND no cursor was emitted - the server did not do any scanning and
    ///   short-circuited. Typically implies that the cursors for the request fell outside the
    ///   available range.
    pub fn has_more(&self) -> bool {
        use grpc_alpha::QueryEndReason as R;
        match self.end_reason {
            None => true,
            Some(R::Unspecified | R::ItemLimit | R::ScanLimit) => true,
            Some(R::LedgerTip | R::CheckpointBound) => false,
            Some(R::CursorBound) => self.last_cursor.is_some(),
            // `QueryEndReason` is non exhaustive — conservatively `true` if a
            // future variant slips past `apply()`'s `unwrap_or(Unspecified)`.
            Some(_) => true,
        }
    }

    /// The latest cursor observed in the direction of pagination. Expect `Some` whenever
    /// `has_more()` is true.
    pub fn next_cursor(&self) -> Option<&Bytes> {
        if self.has_more() {
            Some(
                self.last_cursor
                    .as_ref()
                    .expect("invariant: has_more implies last_cursor is Some"),
            )
        } else {
            self.last_cursor.as_ref()
        }
    }

    /// Fold one frame into the page. `first_cursor` is set once to the first cursor-bearing frame.
    /// `last_cursor` continuously tracks the latest cursor-bearing frame.
    ///
    /// Returns `true` when the frame is the terminal `QueryEnd`.
    fn apply(&mut self, frame: FrameKind<I>) -> bool {
        match frame {
            FrameKind::Item { item, cursor } => {
                if cursor.is_some() {
                    if self.first_cursor.is_none() {
                        self.first_cursor = cursor.clone();
                    }
                    self.last_cursor = cursor;
                }
                self.items.push(item);
            }
            FrameKind::Watermark { cursor } => {
                if cursor.is_some() {
                    if self.first_cursor.is_none() {
                        self.first_cursor = cursor.clone();
                    }
                    self.last_cursor = cursor;
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
            first_cursor: None,
            last_cursor: None,
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
            // Server closed the stream before sending an `End` frame. Fall-through to the post-loop
            // check.
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
                ensure!(
                    !page.items.is_empty() || page.last_cursor.is_some(),
                    "{rpc_name} stream {:?} with no progress: {}",
                    status.code(),
                    status.message(),
                );
                break;
            }
            Some(Err(status)) => {
                bail!("{rpc_name} stream error: {}", status.message());
            }
        }
    }

    // If `has_more() promises further data, the cursor to resume from must be present.`
    ensure!(
        !page.has_more() || page.last_cursor.is_some(),
        "{rpc_name}: server reported more results but did not provide resume cursor — cannot continue",
    );

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
        // `first_cursor` latches on the first cursor-bearing frame (the item at `c1`) and stays
        // there. `last_cursor` advances on every cursor-bearing frame — item OR standalone watermark
        // — so it went `c1` → `c2` (the watermark between items) → `c3` (the last item).
        assert_eq!(page.first_cursor.as_deref(), Some(b"c1".as_ref()));
        assert_eq!(page.last_cursor.as_deref(), Some(b"c3".as_ref()));
        assert_eq!(page.end_reason, Some(grpc_alpha::QueryEndReason::ItemLimit));
    }

    #[test]
    fn standalone_watermark_advances_cursor_without_items() {
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(watermark_response(b"w1").into());
        page.apply(watermark_response(b"w2").into());
        page.apply(end_response(grpc_alpha::QueryEndReason::LedgerTip).into());

        assert!(page.items.is_empty());
        assert_eq!(page.first_cursor.as_deref(), Some(b"w1".as_ref()));
        assert_eq!(page.last_cursor.as_deref(), Some(b"w2".as_ref()));
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
    fn first_cursor_latches_on_first_frame_and_does_not_update() {
        // Multiple cursor-bearing frames: `first_cursor` should remain at the first one,
        // `last_cursor` should track the latest.
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(watermark_response(b"w1").into());
        page.apply(item_response(b"c2").into());
        page.apply(watermark_response(b"w3").into());
        page.apply(item_response(b"c4").into());

        assert_eq!(page.first_cursor.as_deref(), Some(b"w1".as_ref()));
        assert_eq!(page.last_cursor.as_deref(), Some(b"c4".as_ref()));
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
    fn has_more_false_on_authoritative_terminals() {
        // `LedgerTip` and `CheckpointBound` are unconditional terminals — no data past tip /
        // outside the client's cp scope. `CursorBound` with no tracked cursor is the
        // short-circuit case (range collapsed at request resolution).
        for reason in [
            grpc_alpha::QueryEndReason::CheckpointBound,
            grpc_alpha::QueryEndReason::LedgerTip,
            grpc_alpha::QueryEndReason::CursorBound,
        ] {
            let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
            page.apply(end_response(reason).into());
            assert!(!page.has_more(), "expected !has_more for {reason:?}");
        }
    }

    #[test]
    fn has_more_true_on_cursor_bound_with_tracked_cursor() {
        let mut page: StreamPage<grpc_alpha::TransactionItem> = StreamPage::default();
        page.apply(item_response(b"c1").into());
        page.apply(end_response(grpc_alpha::QueryEndReason::CursorBound).into());

        assert_eq!(page.last_cursor.as_deref(), Some(b"c1".as_ref()));
        assert!(
            page.has_more(),
            "CursorBound with tracked cursor should not be terminal"
        );
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
        assert_eq!(page.first_cursor, None);
        assert_eq!(page.last_cursor, None);
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
        assert_eq!(page.last_cursor.as_deref(), Some(b"c2".as_ref()));
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
        assert_eq!(page.last_cursor.as_deref(), Some(b"c1".as_ref()));
        assert_eq!(page.end_reason, None);
    }
}
