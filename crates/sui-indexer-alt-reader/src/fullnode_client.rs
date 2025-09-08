// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{anyhow, Context};
use prometheus::Registry;
use sui_rpc_api::client::{Client, TransactionExecutionResponse};
use sui_types::signature::GenericSignature;
use sui_types::transaction::{Transaction, TransactionData};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::metrics::FullnodeClientMetrics;

/// Like `anyhow::bail!`, but returns this module's `Error` type, not `anyhow::Error`.
macro_rules! bail {
    ($e:expr) => {
        return Err(Error::Internal(anyhow!($e)));
    };
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct FullnodeArgs {
    /// gRPC URL for full node operations such as executeTransaction and simulateTransaction.
    #[clap(long)]
    pub fullnode_rpc_url: Option<String>,
}

/// A client for executing transactions via the full node gRPC service.
#[derive(Clone)]
pub struct FullnodeClient {
    client: Option<Client>,
    metrics: Arc<FullnodeClientMetrics>,
    cancel: CancellationToken,
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
        cancel: CancellationToken,
    ) -> Result<Self, Error> {
        let client = if let Some(url) = &args.fullnode_rpc_url {
            Some(Client::new(url).context("Failed to create gRPC client")?)
        } else {
            None
        };

        let metrics = FullnodeClientMetrics::new(prefix, registry);

        Ok(Self {
            client,
            metrics,
            cancel,
        })
    }

    /// Execute a transaction on the Sui network via gRPC.
    #[instrument(skip(self, transaction_data, signatures), level = "debug")]
    pub async fn execute_transaction(
        &self,
        transaction_data: TransactionData,
        signatures: Vec<GenericSignature>,
    ) -> Result<TransactionExecutionResponse, Error> {
        let transaction = Transaction::from_generic_sig_data(transaction_data, signatures);

        self.request("execute_transaction", |client| async move {
            client.execute_transaction(&transaction).await
        })
        .await
    }

    async fn request<F, Fut>(
        &self,
        method: &str,
        response: F,
    ) -> Result<TransactionExecutionResponse, Error>
    where
        F: FnOnce(Client) -> Fut,
        Fut: std::future::Future<Output = Result<TransactionExecutionResponse, tonic::Status>>,
    {
        let Some(client) = self.client.clone() else {
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

        let response = tokio::select! {
            _ = self.cancel.cancelled() => {
                bail!("Request cancelled");
            }

            r = response(client) => {
                r.map_err(Error::from)
            }
        };

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
