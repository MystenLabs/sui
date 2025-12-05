// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context;
use async_graphql::dataloader::DataLoader;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_types::{
    effects::TransactionEffects, event::Event, messages_checkpoint::CheckpointSummary,
    signature::GenericSignature, transaction::TransactionData,
};
use tonic::transport::{Channel, ClientTlsConfig, Uri};

#[derive(clap::Args, Debug, Clone, Default)]
pub struct LedgerGrpcArgs {
    /// Timeout for gRPC statements to the ledger service, in milliseconds.
    #[arg(long)]
    pub ledger_grpc_statement_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CheckpointedTransaction {
    pub effects: Box<TransactionEffects>,
    pub events: Option<Vec<Event>>,
    pub transaction_data: Box<TransactionData>,
    pub signatures: Vec<GenericSignature>,
    pub timestamp_ms: Option<u64>,
    pub cp_sequence_number: Option<u64>,
}

/// A reader backed by gRPC LedgerService (sui-kv-rpc).
///
/// This connects to archival service that implements the same LedgerService gRPC interface
/// as fullnode, but is backed by Bigtable for serving historical data.
#[derive(Clone)]
pub struct LedgerGrpcReader(pub(crate) LedgerServiceClient<Channel>);

impl LedgerGrpcArgs {
    pub fn statement_timeout(&self) -> Option<std::time::Duration> {
        self.ledger_grpc_statement_timeout_ms
            .map(Duration::from_millis)
    }
}

impl LedgerGrpcReader {
    pub async fn new(uri: Uri, args: LedgerGrpcArgs) -> anyhow::Result<Self> {
        let tls_config = ClientTlsConfig::new().with_native_roots();

        let mut endpoint = Channel::builder(uri);
        if let Some(timeout) = args.statement_timeout() {
            endpoint = endpoint.timeout(timeout);
        }
        let channel = endpoint.tls_config(tls_config)?.connect_lazy();

        let client = LedgerServiceClient::new(channel.clone());
        Ok(Self(client))
    }

    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    pub async fn checkpoint_watermark(&self) -> anyhow::Result<CheckpointSummary> {
        use prost_types::FieldMask;
        use sui_rpc::field::FieldMaskUtil;
        use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;

        let request =
            GetCheckpointRequest::default().with_read_mask(FieldMask::from_paths(["summary.bcs"]));

        let response = self
            .0
            .clone()
            .get_checkpoint(request)
            .await
            .context("Failed to get latest checkpoint")?;

        let checkpoint = response
            .into_inner()
            .checkpoint
            .context("No checkpoint returned")?;

        checkpoint
            .summary
            .as_ref()
            .and_then(|s| s.bcs.as_ref())
            .context("Missing summary.bcs")?
            .deserialize()
            .context("Failed to deserialize checkpoint summary")
    }
}
