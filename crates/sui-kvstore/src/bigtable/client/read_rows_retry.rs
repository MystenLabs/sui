// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! ReadRows attempts bounded by a response gap timeout, retried on another channel and resumed
//! after the last row delivered to the caller.
//!
//! A retried attempt asks only for rows beyond the last delivered row key in scan order (keys
//! after it for forward reads, before it for reversed reads), with `rows_limit` reduced by the
//! rows already delivered, so the caller sees every row exactly once. This relies on BigTable
//! returning rows in key order, as the Java (`ReadRowsResumptionStrategy`) and Go
//! (`retainRowsAfter`) clients do.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rand::Rng;
use tonic::Code;
use tonic::Status;
use tonic::Streaming;
use tracing::warn;

use super::AuthChannel;
use super::BigtableInternalClient;
use super::ChannelPool;
use super::DEFAULT_CHANNEL_TIMEOUT;
use super::channel_pool::ChannelId;
use super::channel_pool::ChannelRouting;
use super::channel_pool::is_quarantine_cancellation;
use super::report_bt_stats_inner;
use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::ReadRowsRequest;
use crate::bigtable::proto::bigtable::v2::ReadRowsResponse;
use crate::bigtable::proto::bigtable::v2::RequestStats;
use crate::bigtable::proto::bigtable::v2::RowRange;
use crate::bigtable::proto::bigtable::v2::RowSet;
use crate::bigtable::proto::bigtable::v2::row_range::EndKey;
use crate::bigtable::proto::bigtable::v2::row_range::StartKey;

const RETRY_BACKOFF_INITIAL: Duration = Duration::from_millis(10);
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(1);

/// Gap timeout and attempt budget for ReadRows calls.
#[derive(Clone, Copy, Debug)]
pub struct ReadRowsRetryConfig {
    /// Longest the caller may wait, while it is waiting on the stream, for the next step of an
    /// attempt: the response headers, timed from when the request is issued, then each response
    /// message. An attempt that exceeds it is abandoned and retried on another channel, and new
    /// requests prefer other channels for a few seconds. The final attempt has no gap timeout,
    /// so a read that is slow on the server still completes. The channel timeout still ends
    /// every attempt's wait for response headers, so a stall before the headers is retried only
    /// when the channel timeout is the longer of the two.
    pub gap_timeout: Duration,
    /// Attempts per ReadRows call, including the first.
    pub max_attempts: u32,
}

/// One ReadRows call across its attempts.
pub(super) struct ResumableReadRows {
    client: BigtableInternalClient<AuthChannel<ChannelPool>>,
    /// The request as the caller issued it; each attempt sends the part not yet delivered.
    request: ReadRowsRequest,
    config: ReadRowsRetryConfig,
    attempts_started: u32,
    /// Channels earlier attempts failed on; later attempts avoid them.
    failed_channels: Vec<ChannelId>,
    routing: ChannelRouting,
    /// Why the previous attempt failed, while the retry it calls for has not started.
    retry_reason: Option<RetryReason>,
    last_delivered_key: Option<Bytes>,
    rows_delivered: i64,
    metrics: Option<Arc<KvMetrics>>,
    client_name: String,
    table_name: String,
}

/// Why an attempt ended before its response did.
pub(super) enum AttemptFailure {
    GapTimeout(WaitPhase),
    Status(Status),
}

/// What an attempt was waiting for when its gap timeout expired.
#[derive(Clone, Copy)]
pub(super) enum WaitPhase {
    Headers,
    Body,
}

/// Why an attempt is retried; also the `reason` label of `kv_bt_read_rows_retries_total`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RetryReason {
    GapTimeout,
    Quarantine,
    /// BigTable answered `Unavailable` or refused the stream, or the connection was lost.
    Other,
}

impl ResumableReadRows {
    pub(super) fn new(
        client: BigtableInternalClient<AuthChannel<ChannelPool>>,
        request: ReadRowsRequest,
        config: ReadRowsRetryConfig,
        metrics: Option<Arc<KvMetrics>>,
        client_name: String,
        table_name: String,
    ) -> Self {
        Self {
            client,
            request,
            config,
            attempts_started: 0,
            failed_channels: Vec::new(),
            routing: ChannelRouting::default(),
            retry_reason: None,
            last_delivered_key: None,
            rows_delivered: 0,
            metrics,
            client_name,
            table_name,
        }
    }

    /// Start an attempt for the rows not yet delivered, retrying attempts that fail before
    /// their response headers. `None` when every requested row has already been delivered.
    pub(super) async fn open(&mut self) -> Result<Option<Streaming<ReadRowsResponse>>, Status> {
        let request = self.remaining_request();
        self.open_from(request).await
    }

    /// Abandon the current attempt after `failure` and start the next one, if the failure is
    /// retryable and the budget allows. A read whose every requested row has already been
    /// delivered ends without error, whatever the failure.
    pub(super) async fn resume(
        &mut self,
        failure: AttemptFailure,
    ) -> Result<Option<Streaming<ReadRowsResponse>>, Status> {
        let Some(request) = self.remaining_request() else {
            // The attempt was still abandoned, so a gap timeout counts even though no retry
            // follows.
            if let AttemptFailure::GapTimeout(phase) = failure {
                self.record_gap_timeout(phase);
            }
            return Ok(None);
        };
        self.prepare_retry(failure)?;
        self.open_from(Some(request)).await
    }

    /// Start an attempt for `request`, the rows not yet delivered, retrying attempts that fail
    /// before their response headers.
    async fn open_from(
        &mut self,
        mut request: Option<ReadRowsRequest>,
    ) -> Result<Option<Streaming<ReadRowsResponse>>, Status> {
        loop {
            let Some(attempt_request) = request.take() else {
                return Ok(None);
            };
            if let Some(reason) = self.retry_reason.take() {
                if reason == RetryReason::Other {
                    tokio::time::sleep(retry_backoff(self.attempts_started)).await;
                }
                if let Some(metrics) = &self.metrics {
                    metrics
                        .kv_bt_read_rows_retries_total
                        .with_label_values(&[&self.client_name, &self.table_name, reason.label()])
                        .inc();
                }
            }
            self.attempts_started += 1;
            self.routing = ChannelRouting::avoiding(self.failed_channels.clone());
            let mut attempt_request = tonic::Request::new(attempt_request);
            attempt_request
                .extensions_mut()
                .insert(self.routing.clone());
            let mut client = self.client.clone();
            let response = client.read_rows(attempt_request);
            let outcome = match self.gap_timeout() {
                None => response.await.map_err(AttemptFailure::Status),
                Some(gap_timeout) => match tokio::time::timeout(gap_timeout, response).await {
                    Ok(response) => response.map_err(AttemptFailure::Status),
                    Err(_) => Err(AttemptFailure::GapTimeout(WaitPhase::Headers)),
                },
            };
            match outcome {
                Ok(response) => return Ok(Some(response.into_inner())),
                Err(failure) => self.prepare_retry(failure)?,
            }
            request = self.remaining_request();
        }
    }

    /// The part of the caller's request not yet delivered; `None` when nothing remains.
    fn remaining_request(&self) -> Option<ReadRowsRequest> {
        remaining_request(
            &self.request,
            self.last_delivered_key.as_ref(),
            self.rows_delivered,
        )
    }

    /// The current attempt's next message, bounded by the gap timeout unless this is the final
    /// attempt. The timeout only runs while this call is pending, so time the caller spends
    /// away from the stream never counts.
    pub(super) async fn next_message(
        &self,
        response: &mut Streaming<ReadRowsResponse>,
    ) -> Result<Option<ReadRowsResponse>, AttemptFailure> {
        match self.gap_timeout() {
            None => response.message().await.map_err(AttemptFailure::Status),
            Some(gap_timeout) => tokio::time::timeout(gap_timeout, response.message())
                .await
                .map_err(|_| AttemptFailure::GapTimeout(WaitPhase::Body))?
                .map_err(AttemptFailure::Status),
        }
    }

    /// Note that the row keyed `row_key` reached the caller.
    pub(super) fn record_delivered(&mut self, row_key: &Bytes) {
        self.last_delivered_key = Some(row_key.clone());
        self.rows_delivered += 1;
    }

    pub(super) fn report_stats(&self, request_stats: &Option<RequestStats>) {
        if let Some(metrics) = &self.metrics {
            report_bt_stats_inner(metrics, &self.client_name, &self.table_name, request_stats);
        }
    }

    fn gap_timeout(&self) -> Option<Duration> {
        (self.attempts_started < self.config.max_attempts).then_some(self.config.gap_timeout)
    }

    /// Account for a failed attempt and decide whether another may start: gap timeouts,
    /// quarantine cancellations, `Unavailable` errors, and attempts lost with their connection
    /// (see [`is_connection_loss`]) are retried within the budget; any other failure is returned.
    fn prepare_retry(&mut self, failure: AttemptFailure) -> Result<(), Status> {
        let reason = match &failure {
            AttemptFailure::GapTimeout(phase) => {
                self.record_gap_timeout(*phase);
                Some(RetryReason::GapTimeout)
            }
            AttemptFailure::Status(status) if is_quarantine_cancellation(status) => {
                Some(RetryReason::Quarantine)
            }
            AttemptFailure::Status(status)
                if status.code() == Code::Unavailable || is_connection_loss(status) =>
            {
                Some(RetryReason::Other)
            }
            AttemptFailure::Status(_) => None,
        };
        let Some(reason) = reason.filter(|_| self.attempts_started < self.config.max_attempts)
        else {
            return Err(failure.into_status());
        };
        if let Some(channel) = self.routing.served_by() {
            self.failed_channels.push(channel);
        }
        self.retry_reason = Some(reason);
        Ok(())
    }

    /// Account for an attempt abandoned after its gap timeout: count it, and penalize the channel
    /// that served it, since the pool can't tell a stall the caller gave up on from a request
    /// that ended normally.
    fn record_gap_timeout(&self, phase: WaitPhase) {
        self.routing.penalize_serving_channel();
        if let Some(metrics) = &self.metrics {
            let phase = match phase {
                WaitPhase::Headers => "headers",
                WaitPhase::Body => "body",
            };
            metrics
                .kv_bt_read_rows_gap_timeouts_total
                .with_label_values(&[&self.client_name, &self.table_name, phase])
                .inc();
        }
    }
}

impl AttemptFailure {
    fn into_status(self) -> Status {
        match self {
            Self::Status(status) => status,
            Self::GapTimeout(_) => {
                Status::deadline_exceeded("timed out waiting for a bigtable ReadRows response")
            }
        }
    }
}

impl RetryReason {
    fn label(self) -> &'static str {
        match self {
            Self::GapTimeout => "gap_timeout",
            Self::Quarantine => "quarantine",
            Self::Other => "other",
        }
    }
}

impl ReadRowsRetryConfig {
    /// Reject settings that defeat the retries: a zero gap timeout abandons every attempt but
    /// the last at once, and zero attempts would leave nothing to run. Warn when
    /// `channel_timeout` (`None` for the client's default), which ends any wait for response
    /// headers, is at most the gap timeout: a ReadRows that stalls before its headers then fails
    /// with `Cancelled` rather than being retried.
    pub fn validate(&self, channel_timeout: Option<Duration>) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.gap_timeout.is_zero(),
            "bigtable-read-rows-gap-timeout-ms must be greater than zero"
        );
        anyhow::ensure!(
            self.max_attempts >= 1,
            "bigtable-read-rows-max-attempts must be at least 1"
        );
        let channel_timeout = channel_timeout.unwrap_or(DEFAULT_CHANNEL_TIMEOUT);
        if self.max_attempts > 1 && channel_timeout <= self.gap_timeout {
            warn!(
                channel_timeout = ?channel_timeout,
                gap_timeout = ?self.gap_timeout,
                "bigtable-channel-timeout-ms does not exceed bigtable-read-rows-gap-timeout-ms, so a ReadRows that stalls before its response headers fails with Cancelled instead of being retried"
            );
        }
        Ok(())
    }
}

impl Default for ReadRowsRetryConfig {
    fn default() -> Self {
        Self {
            gap_timeout: Duration::from_secs(5),
            max_attempts: 3,
        }
    }
}

/// The part of `request` still to be read once `rows_delivered` rows, the last keyed
/// `last_delivered_key`, have reached the caller; `None` when nothing remains.
fn remaining_request(
    request: &ReadRowsRequest,
    last_delivered_key: Option<&Bytes>,
    rows_delivered: i64,
) -> Option<ReadRowsRequest> {
    let mut remaining = request.clone();
    let Some(last_key) = last_delivered_key else {
        return Some(remaining);
    };
    if remaining.rows_limit > 0 {
        remaining.rows_limit -= rows_delivered;
        if remaining.rows_limit <= 0 {
            return None;
        }
    }
    let reversed = remaining.reversed;
    let rows = remaining.rows.get_or_insert_with(RowSet::default);
    if rows.row_keys.is_empty() && rows.row_ranges.is_empty() {
        // An empty row set reads the whole table.
        rows.row_ranges.push(RowRange::default());
    }
    rows.row_keys.retain(|key| {
        if reversed {
            key < last_key
        } else {
            key > last_key
        }
    });
    rows.row_ranges
        .retain_mut(|range| retain_range_beyond(range, last_key, reversed));
    if rows.row_keys.is_empty() && rows.row_ranges.is_empty() {
        return None;
    }
    Some(remaining)
}

/// Pause before an attempt that follows a [`RetryReason::Other`] failure, when BigTable answered
/// `Unavailable` or refused the stream, or the connection was lost: up to 10ms after the first
/// attempt, doubling after each later one, capped at 1s, and jittered so reads that failed
/// together don't retry at the same moment. Quarantine and gap-timeout retries go to another
/// channel at once.
fn retry_backoff(attempts_started: u32) -> Duration {
    let ceiling = RETRY_BACKOFF_INITIAL
        .saturating_mul(2u32.saturating_pow(attempts_started.saturating_sub(1)))
        .min(RETRY_BACKOFF_MAX);
    ceiling.mul_f64(rand::thread_rng().gen_range(0.5..=1.0))
}

/// Whether `status` reports an attempt its connection lost before BigTable finished answering:
/// hyper dropped the request without writing it because the connection closed (tonic reports
/// `Cancelled`), BigTable sent a GOAWAY before the response started (tonic reports `Internal` or
/// another code, by GOAWAY reason), or the connection failed under the open stream, for example
/// by ending without a GOAWAY (an h2 I/O error; tonic reports `Unknown`). Either way the read can
/// be resumed elsewhere.
fn is_connection_loss(status: &Status) -> bool {
    let mut source = std::error::Error::source(status);
    while let Some(error) = source {
        if let Some(h2_error) = error.downcast_ref::<h2::Error>() {
            return h2_error.is_io() || (h2_error.is_go_away() && h2_error.is_remote());
        }
        // hyper reports a request as canceled only when it drops it unsent.
        if let Some(hyper_error) = error.downcast_ref::<hyper::Error>()
            && hyper_error.is_canceled()
        {
            return true;
        }
        source = error.source();
    }
    false
}

/// Narrow `range` to the keys strictly beyond `last_key` in scan order; `false` when none remain.
fn retain_range_beyond(range: &mut RowRange, last_key: &Bytes, reversed: bool) -> bool {
    if reversed {
        if start_key(range).is_some_and(|start| start >= last_key) {
            return false;
        }
        if end_key(range).is_none_or(|end| end >= last_key) {
            range.end_key = Some(EndKey::EndKeyOpen(last_key.clone()));
        }
    } else {
        if end_key(range).is_some_and(|end| end <= last_key) {
            return false;
        }
        if start_key(range).is_none_or(|start| start <= last_key) {
            range.start_key = Some(StartKey::StartKeyOpen(last_key.clone()));
        }
    }
    true
}

/// The range's start key; `None` when the range is unbounded below, which BigTable also assumes
/// for an empty key.
fn start_key(range: &RowRange) -> Option<&Bytes> {
    match &range.start_key {
        Some(StartKey::StartKeyClosed(key) | StartKey::StartKeyOpen(key)) if !key.is_empty() => {
            Some(key)
        }
        _ => None,
    }
}

/// The range's end key; `None` when the range is unbounded above, which BigTable also assumes
/// for an empty key.
fn end_key(range: &RowRange) -> Option<&Bytes> {
    match &range.end_key {
        Some(EndKey::EndKeyClosed(key) | EndKey::EndKeyOpen(key)) if !key.is_empty() => Some(key),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use prometheus::Registry;
    use tonic::Status;

    use super::*;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::client::ChannelPool;
    use crate::bigtable::client::PoolConfig;
    use crate::bigtable::client::raw_h2_server::RawH2Server;
    use crate::bigtable::client::raw_h2_server::ReadRowsMode;
    use crate::bigtable::metrics::KvMetrics;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::mock_server::ReadRowsCall;
    use crate::bigtable::mock_server::ReadRowsFault;
    use crate::bigtable::mock_server::ReadRowsInterruption;

    const TABLE: &str = "t";

    type Rows = Vec<(Bytes, Vec<(Bytes, Bytes)>)>;

    fn key(i: usize) -> Bytes {
        Bytes::from(format!("k{i:04}"))
    }

    fn rows(keys: &[usize]) -> Rows {
        keys.iter()
            .map(|&i| {
                let cells = vec![
                    (Bytes::from_static(b"a"), key(i)),
                    (Bytes::from_static(b"b"), key(i)),
                ];
                (key(i), cells)
            })
            .collect()
    }

    fn sorted_cells(mut rows: Rows) -> Rows {
        for (_, cells) in &mut rows {
            cells.sort();
        }
        rows
    }

    fn keys_request(keys: &[usize], rows_limit: i64) -> ReadRowsRequest {
        ReadRowsRequest {
            table_name: format!("projects/emulator/instances/test/tables/{TABLE}"),
            rows_limit,
            rows: Some(RowSet {
                row_keys: keys.iter().map(|&i| key(i)).collect(),
                row_ranges: vec![],
            }),
            ..ReadRowsRequest::default()
        }
    }

    fn range_request(
        first: usize,
        last: usize,
        rows_limit: i64,
        reversed: bool,
    ) -> ReadRowsRequest {
        ReadRowsRequest {
            table_name: format!("projects/emulator/instances/test/tables/{TABLE}"),
            rows_limit,
            rows: Some(RowSet {
                row_keys: vec![],
                row_ranges: vec![RowRange {
                    start_key: Some(StartKey::StartKeyClosed(key(first))),
                    end_key: Some(EndKey::EndKeyClosed(key(last))),
                }],
            }),
            reversed,
            ..ReadRowsRequest::default()
        }
    }

    struct Fixture {
        mock: MockBigtableServer,
        client: BigTableClient,
        metrics: Arc<KvMetrics>,
        _server: tokio::task::JoinHandle<()>,
    }

    impl Fixture {
        /// A mock holding rows `0..10` and a single-channel client reading from it.
        async fn new(retry: ReadRowsRetryConfig) -> Self {
            let mock = MockBigtableServer::new();
            for i in 0..10 {
                mock.insert_row(TABLE, key(i), [("a", key(i)), ("b", key(i))])
                    .await;
            }
            let (addr, server) = mock.start().await.unwrap();
            let mut client =
                BigTableClient::new_for_host(addr.to_string(), "test".into(), "test", false)
                    .await
                    .unwrap()
                    .with_read_rows_retry(retry);
            let metrics = KvMetrics::new(&Registry::new());
            client.metrics = Some(metrics.clone());
            Self {
                mock,
                client,
                metrics,
                _server: server,
            }
        }

        async fn interrupt_after(&self, after_rows: usize, fault: ReadRowsFault) {
            self.mock
                .interrupt_next_read_rows(ReadRowsInterruption { after_rows, fault })
                .await;
        }

        async fn read(&mut self, request: ReadRowsRequest) -> anyhow::Result<Rows> {
            self.client
                .read_rows(request, TABLE)
                .await
                .map(sorted_cells)
        }

        /// Row keys the mock selected for each `ReadRows` call, in call order.
        async fn calls(&self) -> Vec<Vec<Bytes>> {
            self.mock
                .read_rows_calls()
                .await
                .into_iter()
                .map(|ReadRowsCall { row_keys, .. }| row_keys)
                .collect()
        }

        fn retries(&self, reason: &str) -> u64 {
            self.metrics
                .kv_bt_read_rows_retries_total
                .with_label_values(&["test", TABLE, reason])
                .get()
        }

        fn gap_timeouts(&self, phase: &str) -> u64 {
            self.metrics
                .kv_bt_read_rows_gap_timeouts_total
                .with_label_values(&["test", TABLE, phase])
                .get()
        }
    }

    fn unavailable() -> ReadRowsFault {
        ReadRowsFault::Fail(Status::unavailable("injected"))
    }

    fn keys(indices: &[usize]) -> Vec<Bytes> {
        indices.iter().map(|&i| key(i)).collect()
    }

    #[tokio::test]
    async fn resumed_key_set_read_skips_delivered_keys_and_honors_rows_limit() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig::default()).await;

        // Row 2's first chunk arrives before the failure and is discarded; the retry asks for
        // the keys after row 1 and the two rows the limit still allows.
        fixture.interrupt_after(2, unavailable()).await;
        let read = fixture.read(keys_request(&[0, 1, 2, 3, 4, 5], 4)).await;
        assert_eq!(read.unwrap(), rows(&[0, 1, 2, 3]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[0, 1, 2, 3]), keys(&[2, 3])]
        );
        assert_eq!(fixture.retries("other"), 1);

        // Without a limit, every remaining key is requested once.
        fixture.mock.clear_read_rows_calls().await;
        fixture.interrupt_after(3, unavailable()).await;
        let read = fixture.read(keys_request(&[0, 1, 2, 3, 4, 5], 0)).await;
        assert_eq!(read.unwrap(), rows(&[0, 1, 2, 3, 4, 5]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[0, 1, 2, 3, 4, 5]), keys(&[3, 4, 5])]
        );
    }

    #[tokio::test]
    async fn resumed_forward_scan_starts_after_the_last_delivered_row() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig::default()).await;

        fixture.interrupt_after(2, unavailable()).await;
        let read = fixture.read(range_request(1, 6, 0, false)).await;
        assert_eq!(read.unwrap(), rows(&[1, 2, 3, 4, 5, 6]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[1, 2, 3, 4, 5, 6]), keys(&[3, 4, 5, 6])]
        );

        fixture.mock.clear_read_rows_calls().await;
        fixture.interrupt_after(2, unavailable()).await;
        let read = fixture.read(range_request(1, 6, 3, false)).await;
        assert_eq!(read.unwrap(), rows(&[1, 2, 3]));
        assert_eq!(fixture.calls().await, vec![keys(&[1, 2, 3]), keys(&[3])]);
    }

    #[tokio::test]
    async fn resumed_reversed_scan_ends_before_the_last_delivered_row() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig::default()).await;

        fixture.interrupt_after(2, unavailable()).await;
        let read = fixture.read(range_request(1, 6, 0, true)).await;
        assert_eq!(read.unwrap(), rows(&[6, 5, 4, 3, 2, 1]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[6, 5, 4, 3, 2, 1]), keys(&[4, 3, 2, 1])]
        );

        fixture.mock.clear_read_rows_calls().await;
        fixture.interrupt_after(1, unavailable()).await;
        let read = fixture.read(range_request(1, 6, 4, true)).await;
        assert_eq!(read.unwrap(), rows(&[6, 5, 4, 3]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[6, 5, 4, 3]), keys(&[5, 4, 3])]
        );
    }

    #[tokio::test]
    async fn fully_delivered_reads_end_without_another_request() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig::default()).await;

        // Every requested row arrives before the stream fails, so nothing is left to retry:
        // whether the limit is reached or, without a limit, the keys or ranges are exhausted.
        // Sending a retry with nothing left would leave an empty row set, which BigTable reads
        // as the whole table.
        let requests = [
            (keys_request(&[0, 1, 2], 3), rows(&[0, 1, 2])),
            (keys_request(&[0, 1, 2], 0), rows(&[0, 1, 2])),
            (range_request(1, 3, 0, false), rows(&[1, 2, 3])),
            (range_request(1, 3, 0, true), rows(&[3, 2, 1])),
        ];
        for (request, expected) in requests {
            fixture.mock.clear_read_rows_calls().await;
            fixture.interrupt_after(3, unavailable()).await;
            assert_eq!(fixture.read(request).await.unwrap(), expected);
            assert_eq!(fixture.calls().await.len(), 1);
        }
        assert_eq!(fixture.retries("other"), 0);

        // That holds on the final attempt too, where a failure is otherwise returned.
        let mut fixture = Fixture::new(ReadRowsRetryConfig {
            max_attempts: 1,
            ..ReadRowsRetryConfig::default()
        })
        .await;
        fixture.interrupt_after(3, unavailable()).await;
        let read = fixture.read(keys_request(&[0, 1, 2], 0)).await;
        assert_eq!(read.unwrap(), rows(&[0, 1, 2]));
    }

    #[tokio::test]
    async fn non_retryable_errors_and_exhausted_attempts_surface_to_the_caller() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig::default()).await;

        fixture
            .interrupt_after(1, ReadRowsFault::Fail(Status::invalid_argument("bad")))
            .await;
        let error = fixture.read(keys_request(&[0, 1], 2)).await.unwrap_err();
        let status = error.downcast_ref::<Status>().unwrap();
        assert_eq!(status.code(), Code::InvalidArgument);
        assert_eq!(fixture.calls().await.len(), 1);

        fixture.mock.clear_read_rows_calls().await;
        for _ in 0..3 {
            fixture.interrupt_after(0, unavailable()).await;
        }
        let error = fixture.read(keys_request(&[0, 1], 2)).await.unwrap_err();
        let status = error.downcast_ref::<Status>().unwrap();
        assert_eq!(status.code(), Code::Unavailable);
        assert_eq!(fixture.calls().await.len(), 3);
    }

    #[tokio::test]
    async fn body_stall_is_retried_after_the_gap_timeout() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig {
            gap_timeout: Duration::from_secs(1),
            max_attempts: 3,
        })
        .await;

        fixture.interrupt_after(2, ReadRowsFault::Stall).await;
        let read = fixture.read(keys_request(&[0, 1, 2, 3], 4)).await;
        assert_eq!(read.unwrap(), rows(&[0, 1, 2, 3]));
        assert_eq!(
            fixture.calls().await,
            vec![keys(&[0, 1, 2, 3]), keys(&[2, 3])]
        );
        assert_eq!(fixture.gap_timeouts("body"), 1);
        assert_eq!(fixture.retries("gap_timeout"), 1);
    }

    #[tokio::test]
    async fn final_attempt_waits_out_a_slow_read_without_a_gap_timeout() {
        let mut fixture = Fixture::new(ReadRowsRetryConfig {
            gap_timeout: Duration::from_millis(200),
            max_attempts: 2,
        })
        .await;

        // Every row takes 400ms, longer than the gap timeout: the first attempt is abandoned,
        // and the final attempt has no gap timeout, so it returns every row.
        let row_latency = Duration::from_millis(400);
        fixture.mock.set_read_rows_latency(row_latency).await;
        let started_at = tokio::time::Instant::now();
        let read = fixture.read(keys_request(&[0, 1, 2], 3)).await;
        assert_eq!(read.unwrap(), rows(&[0, 1, 2]));
        assert!(started_at.elapsed() >= row_latency * 3);
        // On a stalled test machine the first attempt can be abandoned before the server records
        // it, and can time out waiting for response headers rather than for a row.
        let calls = fixture.calls().await;
        assert!(matches!(calls.len(), 1 | 2), "{calls:?}");
        assert!(calls.iter().all(|call| *call == keys(&[0, 1, 2])));
        assert_eq!(
            fixture.gap_timeouts("headers") + fixture.gap_timeouts("body"),
            1
        );
        assert_eq!(fixture.retries("gap_timeout"), 1);
    }

    fn row_set(row_keys: &[&str], row_ranges: Vec<RowRange>) -> Option<RowSet> {
        Some(RowSet {
            row_keys: row_keys
                .iter()
                .map(|key| Bytes::copy_from_slice(key.as_bytes()))
                .collect(),
            row_ranges,
        })
    }

    fn row_range(start_key: Option<StartKey>, end_key: Option<EndKey>) -> RowRange {
        RowRange { start_key, end_key }
    }

    fn b(key: &str) -> Bytes {
        Bytes::copy_from_slice(key.as_bytes())
    }

    #[test]
    fn remaining_request_keeps_only_what_lies_beyond_the_last_delivered_key() {
        use EndKey::EndKeyClosed as ThroughEnd;
        use EndKey::EndKeyOpen as BeforeEnd;
        use StartKey::StartKeyClosed as FromStart;
        use StartKey::StartKeyOpen as AfterStart;
        let last = b("m");
        let request = |rows: Option<RowSet>, rows_limit: i64, reversed: bool| ReadRowsRequest {
            rows,
            rows_limit,
            reversed,
            ..ReadRowsRequest::default()
        };
        let mixed = || {
            row_set(
                &["a", "m", "z"],
                vec![
                    row_range(Some(FromStart(b("a"))), Some(ThroughEnd(b("c")))),
                    row_range(Some(FromStart(b("k"))), Some(BeforeEnd(b("q")))),
                    row_range(Some(AfterStart(b("n"))), None),
                    // An empty key leaves that side of the range unbounded.
                    row_range(Some(FromStart(b(""))), Some(ThroughEnd(b("e")))),
                    row_range(None, None),
                    row_range(Some(FromStart(b("m"))), Some(ThroughEnd(b("m")))),
                ],
            )
        };

        // Forward: keys and ranges after "m".
        let remaining = remaining_request(&request(mixed(), 0, false), Some(&last), 4).unwrap();
        let after_m = Some(AfterStart(last.clone()));
        assert_eq!(
            remaining.rows,
            row_set(
                &["z"],
                vec![
                    row_range(after_m.clone(), Some(BeforeEnd(b("q")))),
                    row_range(Some(AfterStart(b("n"))), None),
                    row_range(after_m.clone(), None),
                ],
            )
        );
        assert_eq!(remaining.rows_limit, 0, "no limit stays no limit");

        // Reversed: keys and ranges before "m".
        let remaining = remaining_request(&request(mixed(), 0, true), Some(&last), 4).unwrap();
        let before_m = Some(BeforeEnd(last.clone()));
        assert_eq!(
            remaining.rows,
            row_set(
                &["a"],
                vec![
                    row_range(Some(FromStart(b("a"))), Some(ThroughEnd(b("c")))),
                    row_range(Some(FromStart(b("k"))), before_m.clone()),
                    row_range(Some(FromStart(b(""))), Some(ThroughEnd(b("e")))),
                    row_range(None, before_m.clone()),
                ],
            )
        );

        // A request without a row set reads the whole table; what's left starts after "m".
        let remaining = remaining_request(&request(None, 0, false), Some(&last), 1).unwrap();
        assert_eq!(remaining.rows, row_set(&[], vec![row_range(after_m, None)]));

        // The limit shrinks by the rows already delivered, and a spent limit ends the read.
        let remaining = remaining_request(&request(mixed(), 10, false), Some(&last), 4).unwrap();
        assert_eq!(remaining.rows_limit, 6);
        assert_eq!(
            remaining_request(&request(mixed(), 4, false), Some(&last), 4),
            None
        );

        // Nothing beyond "m" left to read ends the read.
        let done = row_set(
            &["a", "m"],
            vec![row_range(Some(FromStart(b("a"))), Some(ThroughEnd(b("m"))))],
        );
        assert_eq!(
            remaining_request(&request(done, 0, false), Some(&last), 2),
            None
        );

        // Before any row is delivered, the request goes out unchanged.
        let original = request(mixed(), 3, false);
        assert_eq!(
            remaining_request(&original, None, 0),
            Some(original.clone())
        );
    }

    #[tokio::test]
    async fn remote_goaway_is_retried_on_another_channel() {
        let server = RawH2Server::start().await;
        let config = PoolConfig {
            initial_pool_size: 2,
            min_pool_size: 2,
            max_pool_size: 2,
            health_check: None,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(server.endpoint(), config, None, None);
        pool.connect().await.unwrap();
        let mut client = BigTableClient::for_local_pool(pool, "test", "test", false);
        let metrics = KvMetrics::new(&Registry::new());
        client.metrics = Some(metrics.clone());

        // The first attempt's request is never processed; a graceful GOAWAY then tells the client
        // so, which fails the attempt before any response headers, as BigTable's load shedding
        // does.
        server.set_read_rows_mode(ReadRowsMode::Ignore);
        let read = tokio::spawn(async move {
            client
                .read_rows(keys_request(&[1, 2], 2), TABLE)
                .await
                .unwrap()
        });
        server.wait_for_unanswered_requests(1).await;
        server.set_read_rows_mode(ReadRowsMode::Respond);
        server.goaway_ignoring_connections().await;

        let read = read.await.unwrap();
        let read_keys: Vec<Bytes> = read.into_iter().map(|(key, _)| key).collect();
        assert_eq!(read_keys, keys(&[1, 2]));
        let retries = metrics
            .kv_bt_read_rows_retries_total
            .with_label_values(&["test", TABLE, "other"]);
        assert_eq!(retries.get(), 1);
        assert_eq!(server.connections(), 2);
    }
}
