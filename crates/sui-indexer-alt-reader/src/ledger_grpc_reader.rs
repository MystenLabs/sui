// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use async_graphql::dataloader::DataLoader;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_types::{
    effects::TransactionEffects, event::Event, signature::GenericSignature,
    transaction::TransactionData,
};
use tonic::transport::{Channel, ClientTlsConfig, Uri};

#[derive(clap::Args, Debug, Clone, Default)]
pub struct LedgerGrpcArgs {
    /// gRPC endpoint URL for the ledger service (e.g., archive.mainnet.sui.io)
    #[arg(long)]
    pub ledger_grpc_uri: Option<Uri>,
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

impl LedgerGrpcReader {
    pub async fn new(uri: Uri) -> anyhow::Result<Self> {
        let tls_config = ClientTlsConfig::new().with_native_roots();
        let channel = Channel::builder(uri)
            .tls_config(tls_config)?
            .connect()
            .await
            .context("Failed to connect to gRPC endpoint")?;

        let client = LedgerServiceClient::new(channel.clone());
        Ok(Self(client))
    }

    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }
}
