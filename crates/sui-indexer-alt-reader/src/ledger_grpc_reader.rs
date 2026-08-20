// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use anyhow::ensure;
use async_graphql::dataloader::DataLoader;
use async_graphql::dataloader::Loader;
use bytes::Bytes;
use futures::Stream;
use futures::StreamExt;
use futures::future::try_join_all;
use prometheus::Registry;
use prost_types::FieldMask;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::proto_to_timestamp_ms;
use sui_rpc::proto::sui::rpc::v2 as grpc;
use sui_types::effects::TransactionEffects;
use sui_types::event::Event;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tonic::transport::Uri;
use tracing::warn;

use crate::metrics::GrpcMetricsLayer;

#[derive(clap::Args, Debug, Clone)]
pub struct LedgerGrpcArgs {
    /// Timeout for gRPC statements to the ledger service, in milliseconds.
    #[arg(long)]
    pub ledger_grpc_statement_timeout_ms: Option<u64>,

    /// Maximum gRPC decoding message size for Ledger service responses, in bytes.
    #[arg(long, default_value_t = 32 * 1024 * 1024)]
    pub ledger_grpc_max_decoding_message_size: usize,
}

#[derive(Debug, Clone)]
pub struct CheckpointedTransaction {
    pub effects: Box<TransactionEffects>,
    pub events: Option<Vec<Event>>,
    pub transaction_data: Box<TransactionData>,
    pub signatures: Vec<GenericSignature>,
    pub timestamp_ms: Option<u64>,
    pub cp_sequence_number: Option<u64>,
    pub balance_changes: Vec<grpc::BalanceChange>,
}

/// A reader backed by gRPC LedgerService (sui-kv-rpc).
///
/// This connects to archival service that implements the same LedgerService gRPC interface
/// as fullnode, but is backed by Bigtable for serving historical data.
#[derive(Clone)]
pub struct LedgerGrpcReader {
    client: Client,
    timeout: Option<Duration>,
    max_batch_get_transactions: usize,
    max_batch_get_objects: usize,
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
    pub end_reason: Option<grpc::QueryEndReason>,
}

#[derive(Debug)]
enum FrameKind<T> {
    Frame {
        payload: Option<T>,
        cursor: Option<Bytes>,
        end_reason: Option<grpc::QueryEndReason>,
    },
    /// A frame with none of the known fields set (unknown/future frame kind).
    Unknown,
}

/// Maximum number of transaction digests `LedgerGrpcReader` will put in a single
/// `BatchGetTransactions` call, matching the ledger gRPC/KV-RPC service's own hard cap.
pub const MAX_BATCH_GET_TRANSACTIONS: usize = 200;

/// Maximum number of object keys `LedgerGrpcReader` will put in a single `BatchGetObjects`
/// call, matching the ledger gRPC/KV-RPC service's own hard cap.
pub const MAX_BATCH_GET_OBJECTS: usize = 1000;

/// Implemented by `LedgerGrpcReader` for each key type whose `Loader::load` needs to stay under
/// the ledger service's batch-size limit — `DataLoader`'s own `max_batch_size` is only a dispatch
/// trigger, not a hard cap on how many keys reach a single `Loader::load` call. `load_chunk`
/// supplies the raw, single-chunk fetch and `chunk_size` supplies the limit; `load_chunked`'s
/// default body splits `keys` into chunks, dispatches `load_chunk` concurrently per chunk, and
/// merges the results.
///
/// `pub`, not `pub(crate)`: it's referenced through `LedgerGrpcReader`'s blanket `Loader<K>` impl
/// below (`type Value = <Self as ChunkedLoader<K>>::Value`), and that impl is part of
/// `LedgerGrpcReader`'s public interface since both the type and `Loader` are public.
#[async_trait::async_trait]
pub trait ChunkedLoader<K>
where
    K: Send + Sync + Hash + Eq + Clone + 'static,
{
    type Value: Send + Sync + Clone + 'static;
    type Error: Send + Sync + Clone + 'static;

    fn chunk_size(&self) -> usize;

    async fn load_chunk(&self, keys: &[K]) -> Result<HashMap<K, Self::Value>, Self::Error>;

    async fn load_chunked(&self, keys: &[K]) -> Result<HashMap<K, Self::Value>, Self::Error>
    where
        Self: Sync,
    {
        let limit = self.chunk_size();

        let mut results = HashMap::new();
        for batch in try_join_all(keys.chunks(limit).map(|chunk| self.load_chunk(chunk))).await? {
            results.extend(batch);
        }
        Ok(results)
    }
}

/// Covers every key type `LedgerGrpcReader` implements [`ChunkedLoader`] for. Coherent because
/// `Self` (`LedgerGrpcReader`) is a concrete local type — only `K` is generic — unlike a bare
/// `impl<K, T: ChunkedLoader<K>> Loader<K> for T`, which the orphan rules reject since neither
/// `Loader` (foreign) nor `T` (an uncovered generic) is local.
#[async_trait::async_trait]
impl<K> Loader<K> for LedgerGrpcReader
where
    K: Send + Sync + Hash + Eq + Clone + 'static,
    Self: ChunkedLoader<K>,
{
    type Value = <Self as ChunkedLoader<K>>::Value;
    type Error = <Self as ChunkedLoader<K>>::Error;

    async fn load(&self, keys: &[K]) -> Result<HashMap<K, Self::Value>, Self::Error> {
        self.load_chunked(keys).await
    }
}

impl LedgerGrpcArgs {
    pub fn new(
        statement_timeout_ms: Option<u64>,
        max_decoding_message_size: Option<usize>,
    ) -> Self {
        let defaults = Self::default();
        Self {
            ledger_grpc_statement_timeout_ms: statement_timeout_ms,
            ledger_grpc_max_decoding_message_size: max_decoding_message_size
                .unwrap_or(defaults.ledger_grpc_max_decoding_message_size),
        }
    }

    pub fn statement_timeout(&self) -> Option<std::time::Duration> {
        self.ledger_grpc_statement_timeout_ms
            .map(Duration::from_millis)
    }
}

impl CheckpointedTransaction {
    /// Read mask selecting everything needed to construct a `CheckpointedTransaction` from the gRPC
    /// `ExecutedTransaction` proto.
    pub fn read_mask() -> FieldMask {
        FieldMask::from_paths([
            "transaction.bcs",
            "effects.bcs",
            "events.bcs",
            "signatures.bcs",
            "checkpoint",
            "timestamp",
            "balance_changes",
        ])
    }
}

impl LedgerGrpcReader {
    pub async fn new(
        uri: Uri,
        args: LedgerGrpcArgs,
        prefix: Option<&str>,
        registry: &Registry,
        max_batch_get_transactions: usize,
        max_batch_get_objects: usize,
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

        Ok(Self {
            client,
            timeout,
            max_batch_get_transactions,
            max_batch_get_objects,
        })
    }

    pub(crate) fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    pub(crate) fn max_batch_get_transactions(&self) -> usize {
        self.max_batch_get_transactions
    }

    pub(crate) fn max_batch_get_objects(&self) -> usize {
        self.max_batch_get_objects
    }

    pub async fn checkpoint_watermark(&self) -> anyhow::Result<CheckpointSummary> {
        use grpc::GetCheckpointRequest;
        use prost_types::FieldMask;
        use sui_rpc::field::FieldMaskUtil;

        let request =
            GetCheckpointRequest::default().with_read_mask(FieldMask::from_paths(["summary.bcs"]));

        let response = self.get_checkpoint(request).await?;

        let checkpoint = response.checkpoint.context("No checkpoint returned")?;

        checkpoint
            .summary
            .as_ref()
            .and_then(|s| s.bcs.as_ref())
            .context("Missing summary.bcs")?
            .deserialize()
            .context("Failed to deserialize checkpoint summary")
    }

    /// Resolve a checkpoint digest to its sequence number via the ledger service. Returns `None`
    /// if no checkpoint with that digest is known.
    pub async fn checkpoint_seq_by_digest(
        &self,
        digest: sui_types::digests::CheckpointDigest,
    ) -> anyhow::Result<Option<u64>> {
        use grpc::GetCheckpointRequest;
        use prost_types::FieldMask;
        use sui_rpc::field::FieldMaskUtil;

        let sdk_digest = sui_sdk_types::Digest::new(digest.inner().to_owned());
        let request = GetCheckpointRequest::by_digest(&sdk_digest)
            .with_read_mask(FieldMask::from_paths(["sequence_number"]));

        match self.get_checkpoint(request).await {
            Ok(response) => {
                let checkpoint = response.checkpoint.context("No checkpoint returned")?;
                Ok(checkpoint.sequence_number)
            }
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }

    // Public wrapper methods for gRPC calls with metrics instrumentation

    pub async fn get_checkpoint(
        &self,
        request: grpc::GetCheckpointRequest,
    ) -> Result<grpc::GetCheckpointResponse, tonic::Status> {
        self.client
            .clone()
            .ledger_client()
            .get_checkpoint(self.request(request))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn batch_get_transactions(
        &self,
        request: grpc::BatchGetTransactionsRequest,
    ) -> Result<grpc::BatchGetTransactionsResponse, tonic::Status> {
        self.client
            .clone()
            .ledger_client()
            .batch_get_transactions(self.request(request))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn batch_get_objects(
        &self,
        request: grpc::BatchGetObjectsRequest,
    ) -> Result<grpc::BatchGetObjectsResponse, tonic::Status> {
        self.client
            .clone()
            .ledger_client()
            .batch_get_objects(self.request(request))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn get_transaction(
        &self,
        request: grpc::GetTransactionRequest,
    ) -> Result<grpc::GetTransactionResponse, tonic::Status> {
        self.client
            .clone()
            .ledger_client()
            .get_transaction(self.request(request))
            .await
            .map(|r| r.into_inner())
    }

    pub async fn list_transactions(
        &self,
        request: grpc::ListTransactionsRequest,
    ) -> anyhow::Result<StreamPage<grpc::ExecutedTransaction>> {
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
        request: grpc::ListEventsRequest,
    ) -> anyhow::Result<StreamPage<grpc::Event>> {
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
        use grpc::QueryEndReason as R;
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
        end_reason: Option<grpc::QueryEndReason>,
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

impl TryFrom<&grpc::ExecutedTransaction> for CheckpointedTransaction {
    type Error = anyhow::Error;

    fn try_from(executed: &grpc::ExecutedTransaction) -> anyhow::Result<Self> {
        let full_tx: sui_types::full_checkpoint_content::ExecutedTransaction = executed
            .try_into()
            .context("Failed to convert ExecutedTransaction from proto")?;

        let timestamp_ms = executed
            .timestamp
            .map(proto_to_timestamp_ms)
            .transpose()
            .with_context(|| format!("Failed to parse timestamp {:?}", executed.timestamp))?;

        Ok(Self {
            effects: Box::new(full_tx.effects),
            events: full_tx.events.map(|events| events.data),
            transaction_data: Box::new(full_tx.transaction),
            signatures: full_tx.signatures,
            timestamp_ms,
            cp_sequence_number: executed.checkpoint,
            balance_changes: executed.balance_changes.clone(),
        })
    }
}

impl Default for LedgerGrpcArgs {
    fn default() -> Self {
        Self {
            ledger_grpc_statement_timeout_ms: None,
            ledger_grpc_max_decoding_message_size: 32 * 1024 * 1024,
        }
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

impl TryFrom<grpc::ListTransactionsResponse> for FrameKind<grpc::ExecutedTransaction> {
    type Error = anyhow::Error;

    fn try_from(response: grpc::ListTransactionsResponse) -> anyhow::Result<Self> {
        classify_frame(response.transaction, response.watermark, response.end)
    }
}

impl TryFrom<grpc::ListEventsResponse> for FrameKind<grpc::Event> {
    type Error = anyhow::Error;

    fn try_from(response: grpc::ListEventsResponse) -> anyhow::Result<Self> {
        classify_frame(response.event, response.watermark, response.end)
    }
}

/// Classify a raw list-stream response into a [`FrameKind`], given its payload field. Per-API
/// implementations only select which response field is the payload.
fn classify_frame<T>(
    payload: Option<T>,
    watermark: Option<grpc::Watermark>,
    end: Option<grpc::QueryEnd>,
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
pub(crate) mod test_support {
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use prometheus::Registry;
    use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
    use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
    use sui_rpc::proto::sui::rpc::v2::BatchGetTransactionsRequest;
    use sui_rpc::proto::sui::rpc::v2::BatchGetTransactionsResponse;
    use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerService;
    use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::Request;
    use tonic::Response;
    use tonic::Status;

    use super::LedgerGrpcArgs;
    use super::LedgerGrpcReader;
    use super::MAX_BATCH_GET_OBJECTS;
    use super::MAX_BATCH_GET_TRANSACTIONS;

    /// Starts a [`MockLedgerServer`] and constructs a [`LedgerGrpcReader`]
    /// pointed at it. Shared by every loader's chunking test.
    pub(crate) async fn mock_reader() -> (LedgerGrpcReader, MockLedgerServer, JoinHandle<()>) {
        let mock = MockLedgerServer::new();
        let (addr, server) = mock.start().await.expect("start mock ledger service");
        let reader = LedgerGrpcReader::new(
            format!("http://{addr}").parse().unwrap(),
            LedgerGrpcArgs::default(),
            None,
            &Registry::new(),
            MAX_BATCH_GET_TRANSACTIONS,
            MAX_BATCH_GET_OBJECTS,
        )
        .await
        .expect("construct LedgerGrpcReader");
        (reader, mock, server)
    }

    /// Asserts that `batches` (the digest lists recorded by
    /// [`MockLedgerServer`] across however many gRPC calls a loader made) is
    /// chunked to `limit`, and that their union reconstructs `expected`
    /// exactly, with none lost or duplicated.
    pub(crate) fn assert_chunked(batches: Vec<Vec<String>>, limit: usize, expected: &[String]) {
        assert_eq!(batches.len(), expected.len().div_ceil(limit));
        assert!(batches.iter().all(|batch| batch.len() <= limit));

        let mut requested: Vec<String> = batches.into_iter().flatten().collect();
        requested.sort();
        let mut expected = expected.to_vec();
        expected.sort();
        assert_eq!(requested, expected);
    }

    /// A minimal in-process `LedgerService` that records the shape of every
    /// `BatchGetTransactions`/`BatchGetObjects` call it receives and responds
    /// with an empty result set. Every other RPC falls back to the generated
    /// trait's default `unimplemented` behavior, which is all the chunking
    /// tests that use this need.
    #[derive(Clone, Default)]
    pub(crate) struct MockLedgerServer {
        transaction_batches: Arc<Mutex<Vec<Vec<String>>>>,
        object_batches: Arc<Mutex<Vec<Vec<GetObjectRequest>>>>,
    }

    impl MockLedgerServer {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) async fn start(&self) -> anyhow::Result<(SocketAddr, JoinHandle<()>)> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let addr = listener.local_addr()?;
            let mock = self.clone();
            let handle = tokio::spawn(async move {
                let incoming = TcpListenerStream::new(listener);
                tonic::transport::Server::builder()
                    .add_service(LedgerServiceServer::new(mock))
                    .serve_with_incoming(incoming)
                    .await
                    .ok();
            });
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok((addr, handle))
        }

        pub(crate) fn transaction_batches(&self) -> Vec<Vec<String>> {
            self.transaction_batches.lock().unwrap().clone()
        }

        pub(crate) fn object_batches(&self) -> Vec<Vec<GetObjectRequest>> {
            self.object_batches.lock().unwrap().clone()
        }
    }

    #[tonic::async_trait]
    impl LedgerService for MockLedgerServer {
        async fn batch_get_transactions(
            &self,
            request: Request<BatchGetTransactionsRequest>,
        ) -> Result<Response<BatchGetTransactionsResponse>, Status> {
            self.transaction_batches
                .lock()
                .unwrap()
                .push(request.into_inner().digests);
            Ok(Response::new(BatchGetTransactionsResponse::default()))
        }

        async fn batch_get_objects(
            &self,
            request: Request<BatchGetObjectsRequest>,
        ) -> Result<Response<BatchGetObjectsResponse>, Status> {
            self.object_batches
                .lock()
                .unwrap()
                .push(request.into_inner().requests);
            Ok(Response::new(BatchGetObjectsResponse::default()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Well-formed frame with both `transaction` payload and a cursor-bearing watermark.
    fn item_response(cursor: &[u8]) -> grpc::ListTransactionsResponse {
        let mut watermark = grpc::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut response = grpc::ListTransactionsResponse::default();
        response.transaction = Some(grpc::ExecutedTransaction::default());
        response.watermark = Some(watermark);
        response
    }

    fn watermark_response(cursor: &[u8]) -> grpc::ListTransactionsResponse {
        let mut watermark = grpc::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        let mut response = grpc::ListTransactionsResponse::default();
        response.watermark = Some(watermark);
        response
    }

    fn end_response(reason: grpc::QueryEndReason) -> grpc::ListTransactionsResponse {
        let mut end = grpc::QueryEnd::default();
        end.reason = Some(reason as i32);
        let mut response = grpc::ListTransactionsResponse::default();
        response.end = Some(end);
        response
    }

    fn end_response_with_cursor(
        cursor: &[u8],
        reason: grpc::QueryEndReason,
    ) -> grpc::ListTransactionsResponse {
        let mut response = end_response(reason);
        let mut watermark = grpc::Watermark::default();
        watermark.cursor = Some(Bytes::copy_from_slice(cursor));
        response.watermark = Some(watermark);
        response
    }

    fn frame(r: grpc::ListTransactionsResponse) -> FrameKind<grpc::ExecutedTransaction> {
        r.try_into().expect("test fixture should be well-formed")
    }

    async fn drain_iter(
        responses: Vec<Result<grpc::ListTransactionsResponse, tonic::Status>>,
    ) -> anyhow::Result<StreamPage<grpc::ExecutedTransaction>> {
        drain_list_stream::<_, grpc::ExecutedTransaction, _>(
            "ListTransactions",
            futures::stream::iter(responses),
        )
        .await
    }

    #[test]
    fn drains_items_tracking_latest_cursor_and_end_reason() {
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        page.apply(frame(item_response(b"c1")));
        page.apply(frame(watermark_response(b"w2")));
        let mut last = item_response(b"c3");
        last.end = end_response(grpc::QueryEndReason::ItemLimit).end;
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
        assert_eq!(page.end_reason, Some(grpc::QueryEndReason::ItemLimit));
    }

    #[test]
    fn standalone_watermark_advances_cursor_without_items() {
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        page.apply(frame(watermark_response(b"w1")));
        page.apply(frame(end_response_with_cursor(
            b"w2",
            grpc::QueryEndReason::LedgerTip,
        )));

        assert!(page.items.is_empty());
        assert_eq!(
            page.first_cursor().map(|c| c.as_ref()),
            Some(b"w1".as_ref())
        );
        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"w2".as_ref()));
        assert_eq!(page.end_reason, Some(grpc::QueryEndReason::LedgerTip));
    }

    #[test]
    fn apply_signals_stop_only_on_end_frame() {
        // The bool returned by `apply` is the drain loop's stop signal: `true` means "stop
        // draining," `false` means "keep going." Only frames carrying `end` should signal stop.
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        assert!(!page.apply(frame(item_response(b"c1"))));
        assert!(!page.apply(frame(watermark_response(b"w1"))));
        // Outer message with none of the known fields set → `FrameKind::Unknown` → continue.
        assert!(!page.apply(frame(grpc::ListTransactionsResponse::default())));
        assert!(page.apply(frame(end_response(grpc::QueryEndReason::LedgerTip))));

        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        let mut item_end = item_response(b"c2");
        item_end.end = end_response(grpc::QueryEndReason::ItemLimit).end;
        assert!(page.apply(frame(item_end)));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].cursor.as_ref(), b"c2".as_ref());
        assert_eq!(page.end_reason, Some(grpc::QueryEndReason::ItemLimit));
    }

    #[test]
    fn first_wm_cursor_set_to_first_pre_item_watermark() {
        // `first_wm_cursor` is set when watermark frame observed before items.
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
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
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
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
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
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
            grpc::QueryEndReason::ItemLimit,
            grpc::QueryEndReason::ScanLimit,
        ] {
            let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
            page.apply(frame(end_response(reason)));
            assert!(page.has_more(), "expected has_more for {reason:?}");
        }

        // `end_reason == None` covers both the deadline cut-short case (no end frame
        // received) and any unrecognized / future-added variant — defaulting to "may have more"
        // avoids silent truncation.
        let page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        assert!(page.has_more());
    }

    #[test]
    fn has_more_false_on_authoritative_terminals() {
        // `LedgerTip` and `CheckpointBound` are unconditional terminals — no data past tip /
        // outside the client's cp scope. `CursorBound` with no tracked cursor is the
        // short-circuit case (range collapsed at request resolution).
        for reason in [
            grpc::QueryEndReason::CheckpointBound,
            grpc::QueryEndReason::LedgerTip,
            grpc::QueryEndReason::CursorBound,
        ] {
            let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
            page.apply(frame(end_response(reason)));
            assert!(!page.has_more(), "expected !has_more for {reason:?}");
        }
    }

    #[test]
    fn has_more_true_on_cursor_bound_with_tracked_cursor() {
        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        let mut response = item_response(b"c1");
        response.end = end_response(grpc::QueryEndReason::CursorBound).end;
        page.apply(frame(response));

        assert_eq!(page.last_cursor().map(|c| c.as_ref()), Some(b"c1".as_ref()));
        assert!(
            page.has_more(),
            "CursorBound with tracked cursor should not be terminal"
        );
    }

    #[test]
    fn apply_end_with_unknown_reason_folds_to_unknown() {
        let mut end = grpc::QueryEnd::default();
        end.reason = Some(i32::MAX);
        let mut response = grpc::ListTransactionsResponse::default();
        response.end = Some(end);

        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
        page.apply(frame(response));
        assert_eq!(page.end_reason, Some(grpc::QueryEndReason::Unknown));
    }

    #[test]
    fn apply_unknown_frame_does_not_mutate_page() {
        // Outer message with none of the known fields set classifies to `FrameKind::Unknown`.
        // `apply` should warn but leave items / cursors / end_reason untouched.
        let response = grpc::ListTransactionsResponse::default();

        let mut page: StreamPage<grpc::ExecutedTransaction> = StreamPage::default();
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
        let mut response = grpc::ListTransactionsResponse::default();
        response.transaction = Some(grpc::ExecutedTransaction::default());

        let result: anyhow::Result<FrameKind<grpc::ExecutedTransaction>> = response.try_into();
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
        let mut malformed = grpc::ListTransactionsResponse::default();
        malformed.transaction = Some(grpc::ExecutedTransaction::default());

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
