// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

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
use tracing::instrument;
use url::Url;

use crate::metrics::FullnodeClientMetrics;

#[derive(clap::Args, Debug, Clone)]
pub struct FullnodeArgs {
    /// gRPC URL for full node operations such as executeTransaction and simulateTransaction.
    #[clap(long)]
    pub fullnode_rpc_url: Url,
}

/// A client for executing and simulating transactions via the full node gRPC service.
#[derive(Clone)]
pub struct FullnodeClient {
    execution_client: TransactionExecutionServiceClient<Channel>,
    metrics: Arc<FullnodeClientMetrics>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    GrpcExecutionError(#[from] tonic::Status),
}

impl FullnodeClient {
    pub async fn new(
        prefix: Option<&str>,
        args: FullnodeArgs,
        registry: &Registry,
    ) -> Result<Self, Error> {
        let mut endpoint = Channel::from_shared(args.fullnode_rpc_url.to_string())
            .context("Failed to create channel for gRPC endpoint")?;

        if args.fullnode_rpc_url.scheme() == "https" {
            endpoint = endpoint
                .tls_config(ClientTlsConfig::new().with_native_roots())
                .context("Failed to configure TLS for gRPC endpoint")?;
        }

        let channel = endpoint.connect_lazy();
        let execution_client = TransactionExecutionServiceClient::new(channel);
        let metrics = FullnodeClientMetrics::new(prefix, registry);

        Ok(Self {
            execution_client,
            metrics,
        })
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

        self.request("execute_transaction", |mut client| async move {
            client.execute_transaction(request).await
        })
        .await
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

        self.request("simulate_transaction", |mut client| async move {
            client.simulate_transaction(request).await
        })
        .await
    }

    async fn request<F, Fut, R>(&self, method: &str, response: F) -> Result<R, Error>
    where
        F: FnOnce(TransactionExecutionServiceClient<Channel>) -> Fut,
        Fut: std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>>,
    {
        self.metrics
            .requests_received
            .with_label_values(&[method])
            .inc();

        let _timer = self
            .metrics
            .latency
            .with_label_values(&[method])
            .start_timer();

        let response = response(self.execution_client.clone())
            .await
            .map(|r| r.into_inner())
            .map_err(Into::into);

        if response.is_ok() {
            self.metrics
                .requests_succeeded
                .with_label_values(&[method])
                .inc();
        } else {
            self.metrics
                .requests_failed
                .with_label_values(&[method])
                .inc();
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fn_client(url: &str) -> Result<FullnodeClient, Error> {
        let url = Url::parse(url).unwrap();
        let registry = Registry::new();
        let args = FullnodeArgs {
            fullnode_rpc_url: url,
        };
        FullnodeClient::new(None, args, &registry).await
    }

    #[tokio::test]
    async fn http_url_creates_client() {
        fn_client("http://localhost:9000").await.unwrap();
    }

    #[tokio::test]
    async fn https_url_creates_client() {
        fn_client("https://fn.example.com").await.unwrap();
    }
}
