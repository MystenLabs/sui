// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use prometheus::Registry;
use prost_types::FieldMask;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_types::signature::GenericSignature;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;
use tower::Layer;
use tracing::instrument;
use url::Url;

use crate::metrics::GrpcMetricsLayer;
use crate::metrics::GrpcMetricsService;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct FullnodeArgs {
    /// gRPC URL for full node operations such as executeTransaction and simulateTransaction.
    /// `Option` so the flag stays optional when flattened into a parent args struct.
    #[clap(long)]
    pub(crate) fullnode_rpc_url: Option<Url>,
}

/// A client for executing and simulating transactions via the full node gRPC service.
#[derive(Clone)]
pub struct FullnodeClient {
    execution_client: TransactionExecutionServiceClient<GrpcMetricsService<Channel>>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    GrpcExecutionError(#[from] tonic::Status),
}

impl FullnodeArgs {
    pub fn new(url: Url) -> Self {
        Self {
            fullnode_rpc_url: Some(url),
        }
    }
}

impl FullnodeClient {
    pub async fn new(
        prefix: Option<&str>,
        args: FullnodeArgs,
        registry: &Registry,
    ) -> Result<Option<Self>, Error> {
        let Some(url) = args.fullnode_rpc_url else {
            return Ok(None);
        };

        let mut endpoint = Channel::from_shared(url.to_string())
            .context("Failed to create channel for gRPC endpoint")?;

        if url.scheme() == "https" {
            endpoint = endpoint
                .tls_config(ClientTlsConfig::new().with_native_roots())
                .context("Failed to configure TLS for gRPC endpoint")?;
        }

        let channel = endpoint.connect_lazy();

        let layered = GrpcMetricsLayer::new(prefix.unwrap_or("fullnode"), registry).layer(channel);

        let execution_client = TransactionExecutionServiceClient::new(layered);

        Ok(Some(Self { execution_client }))
    }

    /// Execute a transaction on the Sui network via gRPC.
    #[instrument(skip(self, transaction_data, signatures, read_mask), level = "debug")]
    pub async fn execute_transaction(
        &self,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
        read_mask: FieldMask,
    ) -> Result<proto::ExecuteTransactionResponse, Error> {
        let transaction = Transaction::from_generic_sig_data(transaction_data, signatures);

        let signatures = transaction
            .inner()
            .tx_signatures
            .iter()
            .map(|signature| {
                let mut message = proto::UserSignature::default();
                message.bcs = Some(signature.as_ref().to_vec().into());
                message
            })
            .collect();

        let request = proto::ExecuteTransactionRequest::new({
            let mut tx = proto::Transaction::default();
            tx.bcs = Some(
                proto::Bcs::serialize(&transaction.inner().intent_message.value)
                    .context("Failed to serialize transaction")?,
            );
            tx
        })
        .with_signatures(signatures)
        .with_read_mask(read_mask);

        self.execution_client
            .clone()
            .execute_transaction(request)
            .await
            .map(|r| r.into_inner())
            .map_err(Into::into)
    }

    /// Simulate a transaction on the Sui network via gRPC.
    /// Note: Simulation does not require signatures since the transaction is not committed to the blockchain.
    ///
    /// - `checks_enabled`: If true, enables transaction validation checks during simulation.
    /// - `do_gas_selection`: If true, enables automatic gas coin selection and budget estimation.
    #[instrument(skip(self, transaction, read_mask), level = "debug")]
    pub async fn simulate_transaction(
        &self,
        transaction: proto::Transaction,
        checks_enabled: bool,
        do_gas_selection: bool,
        read_mask: FieldMask,
    ) -> Result<proto::SimulateTransactionResponse, Error> {
        use proto::simulate_transaction_request::TransactionChecks;

        let checks = if checks_enabled {
            TransactionChecks::Enabled
        } else {
            TransactionChecks::Disabled
        };

        let request = proto::SimulateTransactionRequest::new(transaction)
            .with_read_mask(read_mask)
            .with_checks(checks)
            .with_do_gas_selection(do_gas_selection);

        self.execution_client
            .clone()
            .simulate_transaction(request)
            .await
            .map(|r| r.into_inner())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fn_client(url: Option<&str>) -> Result<Option<FullnodeClient>, Error> {
        let registry = Registry::new();
        let args = FullnodeArgs {
            fullnode_rpc_url: url.map(|u| Url::parse(u).unwrap()),
        };
        FullnodeClient::new(None, args, &registry).await
    }

    #[tokio::test]
    async fn no_url_means_not_configured() {
        let client = fn_client(None).await.unwrap();
        assert!(client.is_none());
    }

    #[tokio::test]
    async fn http_url_creates_client() {
        assert!(
            fn_client(Some("http://localhost:9000"))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn https_url_creates_client() {
        assert!(
            fn_client(Some("https://fn.example.com"))
                .await
                .unwrap()
                .is_some()
        );
    }
}
