// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::anyhow;
use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_sdk_types::Address;
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

    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
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

    /// Construct and dry run a PTB to calculate the rewards for a list of staked SUI objects.
    /// Returns a list of u64 guaranteed to match the order of the input staked SUI ids.
    pub async fn calculate_rewards(&self, staked_sui_ids: &[Address]) -> Result<Vec<u64>, Error> {
        let mut ptb = proto::ProgrammableTransaction::default()
            .with_inputs(vec![proto::Input::default().with_object_id("0x5")]);
        let system_object = proto::Argument::new_input(0);

        for id in staked_sui_ids {
            let staked_sui = proto::Argument::new_input(ptb.inputs.len() as u16);
            ptb.inputs.push(proto::Input::default().with_object_id(id));
            ptb.commands.push(
                proto::MoveCall::default()
                    .with_package("0x3")
                    .with_module("sui_system")
                    .with_function("calculate_rewards")
                    .with_arguments(vec![system_object, staked_sui])
                    .into(),
            );
        }

        let transaction = proto::Transaction::default()
            .with_kind(ptb)
            .with_sender("0x0");

        let resp = self
            .simulate_transaction(
                transaction,
                false,
                false,
                FieldMask::from_paths([
                    "command_outputs.return_values.value",
                    "transaction.effects.status",
                ]),
            )
            .await?;

        if !resp.transaction().effects().status().success() {
            return Err(Error::Internal(anyhow!("transaction execution failed")));
        }

        if staked_sui_ids.len() != resp.command_outputs.len() {
            return Err(Error::Internal(anyhow!(
                "missing transaction command_outputs"
            )));
        }

        resp.command_outputs
            .iter()
            .map(|output| {
                // At success, expect every command to guarantee a u64 returned
                let bcs_rewards = output
                    .return_values
                    .first()
                    .and_then(|o| o.value_opt())
                    .ok_or_else(|| Error::Internal(anyhow!("missing rewards bcs")))?;

                bcs::from_bytes::<u64>(bcs_rewards.value())
                    .map_err(|e| Error::Internal(anyhow!("Failed to deserialize rewards: {e}")))
            })
            .collect()
    }

    /// Construct and dry run a PTB to get the corresponding validator addresses for a list of
    /// staking pool ids. Returns a list of validator addresses guaranteed to match the order of the
    /// input pool ids.
    pub async fn get_validator_address_by_pool_id(
        &self,
        pool_ids: &[Address],
    ) -> Result<Vec<Address>, Error> {
        let mut ptb = proto::ProgrammableTransaction::default()
            .with_inputs(vec![proto::Input::default().with_object_id("0x5")]);
        let system_object = proto::Argument::new_input(0);

        for id in pool_ids {
            let pool_id = proto::Argument::new_input(ptb.inputs.len() as u16);
            ptb.inputs
                .push(proto::Input::default().with_pure(id.into_inner().to_vec()));
            ptb.commands.push(
                proto::MoveCall::default()
                    .with_package("0x3")
                    .with_module("sui_system")
                    .with_function("validator_address_by_pool_id")
                    .with_arguments(vec![system_object, pool_id])
                    .into(),
            );
        }

        let transaction = proto::Transaction::default()
            .with_kind(ptb)
            .with_sender("0x0");

        let resp = self
            .simulate_transaction(
                transaction,
                false,
                false,
                FieldMask::from_paths([
                    "command_outputs.return_values.value",
                    "transaction.effects.status",
                ]),
            )
            .await?;

        if !resp.transaction().effects().status().success() {
            return Err(Error::Internal(anyhow!("transaction execution failed")));
        }

        if pool_ids.len() != resp.command_outputs.len() {
            return Err(Error::Internal(anyhow!(
                "Mismatch between transaction inputs and command_outputs"
            )));
        }

        resp.command_outputs
            .iter()
            .map(|output| {
                // Both active and inactive validators are checked, so on success expect every
                // command to have a return address
                let bcs_address = output
                    .return_values
                    .first()
                    .and_then(|o| o.value_opt())
                    .ok_or_else(|| Error::Internal(anyhow!("missing address bcs")))?;

                Address::from_bytes(bcs_address.value())
                    .map_err(|e| Error::Internal(anyhow!("Failed to deserialize address: {e}")))
            })
            .collect()
    }
}

impl From<Error> for crate::error::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Internal(err) => err.into(),
            Error::GrpcExecutionError(status) => status.into(),
        }
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
