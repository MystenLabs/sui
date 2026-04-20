// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_rpc::proto::sui::rpc::v2 as grpc;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_types::effects::TransactionEffects;
use sui_types::event::Event;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;
use tonic::transport::Uri;
use tower::Layer;

use crate::metrics::GrpcMetricsLayer;
use crate::metrics::GrpcMetricsService;

const DEFAULT_MAX_DECODING_MESSAGE_SIZE: usize = 32 * 1024 * 1024;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct LedgerGrpcArgs {
    /// Timeout for gRPC statements to the ledger service, in milliseconds.
    #[arg(long)]
    pub ledger_grpc_statement_timeout_ms: Option<u64>,

    /// Maximum gRPC decoding message size for Ledger service responses, in bytes.
    #[arg(long)]
    pub ledger_grpc_max_decoding_message_size: Option<usize>,
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
    client: LedgerServiceClient<GrpcMetricsService<Channel>>,
    timeout: Option<Duration>,
}

impl LedgerGrpcArgs {
    pub fn statement_timeout(&self) -> Option<std::time::Duration> {
        self.ledger_grpc_statement_timeout_ms
            .map(Duration::from_millis)
    }
}

impl LedgerGrpcReader {
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
        let client =
            LedgerServiceClient::new(layered).max_decoding_message_size(max_decoding_message_size);

        Ok(Self { client, timeout })
    }

    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
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

    // Public wrapper methods for gRPC calls with metrics instrumentation

    pub async fn get_checkpoint(
        &self,
        request: grpc::GetCheckpointRequest,
    ) -> Result<grpc::GetCheckpointResponse, tonic::Status> {
        self.client
            .clone()
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
