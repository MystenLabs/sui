// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context;
use prometheus::Registry;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
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

#[derive(clap::Args, Debug, Clone, Default)]
pub struct FullnodeArgs {
    /// gRPC URL for full node operations such as executeTransaction and simulateTransaction.
    #[clap(long)]
    pub fullnode_rpc_url: Option<Url>,
}

/// A client for executing and simulating transactions via the full node gRPC service.
#[derive(Clone)]
pub struct FullnodeClient {
    execution_client: Option<TransactionExecutionServiceClient<Channel>>,
    metrics: Arc<FullnodeClientMetrics>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Full node client not configured")]
    NotConfigured,

    #[error(transparent)]
    GrpcExecutionError(#[from] tonic::Status),
}

impl FullnodeClient {
    pub async fn new(
        prefix: Option<&str>,
        args: FullnodeArgs,
        registry: &Registry,
    ) -> Result<Self, Error> {
        let execution_client = if let Some(url) = &args.fullnode_rpc_url {
            let mut endpoint = Channel::from_shared(url.to_string())
                .context("Failed to create channel for gRPC endpoint")?;

            if url.scheme() == "https" {
                endpoint = endpoint
                    .tls_config(ClientTlsConfig::new().with_native_roots())
                    .context("Failed to configure TLS for gRPC endpoint")?;
            }

            let channel = endpoint.connect_lazy();
            Some(TransactionExecutionServiceClient::new(channel))
        } else {
            None
        };

        let metrics = FullnodeClientMetrics::new(prefix, registry);

        Ok(Self {
            execution_client,
            metrics,
        })
    }

    /// Execute a transaction on the Sui network via gRPC.
    #[instrument(skip(self, transaction_data, signatures), level = "debug")]
    pub async fn execute_transaction(
        &self,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
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
        .with_read_mask(FieldMask::from_paths([
            "effects",
            "transaction",
            "events.bcs",
            "balance_changes",
            "objects.objects.bcs",
        ]));

        self.request(
            "execute_transaction",
            self.execution_client.clone(),
            |mut client| async move { client.execute_transaction(request).await },
        )
        .await
    }

    /// Simulate a transaction on the Sui network via gRPC.
    /// Note: Simulation does not require signatures since the transaction is not committed to the blockchain.
    ///
    /// - `checks_enabled`: If true, enables transaction validation checks during simulation. Defaults to true.
    /// - `do_gas_selection`: If true, enables automatic gas coin selection and budget estimation. Defaults to false.
    #[instrument(skip(self, transaction), level = "debug")]
    pub async fn simulate_transaction(
        &self,
        transaction: proto::Transaction,
        checks_enabled: bool,
        do_gas_selection: bool,
    ) -> Result<proto::SimulateTransactionResponse, Error> {
        use proto::simulate_transaction_request::TransactionChecks;

        let checks = if checks_enabled {
            TransactionChecks::Enabled
        } else {
            TransactionChecks::Disabled
        };

        let request = proto::SimulateTransactionRequest::new(transaction)
            .with_read_mask(FieldMask::from_paths([
                "transaction.effects",
                "transaction.transaction",
                "transaction.events.bcs",
                "transaction.balance_changes",
                "transaction.objects.objects.bcs",
                "transaction.transaction.bcs",
                "command_outputs",
            ]))
            .with_checks(checks)
            .with_do_gas_selection(do_gas_selection);

        self.request(
            "simulate_transaction",
            self.execution_client.clone(),
            |mut client| async move { client.simulate_transaction(request).await },
        )
        .await
    }

    async fn request<C, F, Fut, R>(
        &self,
        method: &str,
        client: Option<C>,
        response: F,
    ) -> Result<R, Error>
    where
        F: FnOnce(C) -> Fut,
        Fut: std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>>,
    {
        let Some(client) = client else {
            return Err(Error::NotConfigured);
        };

        self.metrics
            .requests_received
            .with_label_values(&[method])
            .inc();

        let _timer = self
            .metrics
            .latency
            .with_label_values(&[method])
            .start_timer();

        let response = response(client)
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

    async fn fn_client(url: Option<&str>) -> Result<FullnodeClient, Error> {
        let args = FullnodeArgs {
            fullnode_rpc_url: url.map(|u| Url::parse(u).unwrap()),
        };
        let registry = Registry::new();
        FullnodeClient::new(None, args, &registry).await
    }

    #[tokio::test]
    async fn no_url_means_not_configured() {
        let client = fn_client(None).await.unwrap();
        assert!(client.execution_client.is_none());
    }

    #[tokio::test]
    async fn http_url_creates_client() {
        assert!(
            fn_client(Some("http://localhost:9000"))
                .await
                .unwrap()
                .execution_client
                .is_some()
        );
    }

    #[tokio::test]
    async fn https_url_creates_client() {
        assert!(
            fn_client(Some("https://fn.example.com"))
                .await
                .unwrap()
                .execution_client
                .is_some()
        );
    }
}
