// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use anyhow::Context;
use async_graphql::dataloader::DataLoader;
use async_graphql::dataloader::Loader;
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

    /// Create a gRPC request, optionally with the grpc-timeout header if configured.
    fn request<T>(&self, input: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(input);
        if let Some(timeout) = self.timeout {
            request.set_timeout(timeout);
        }
        request
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
