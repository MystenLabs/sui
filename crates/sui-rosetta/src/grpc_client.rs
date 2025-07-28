// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use sui_rpc::proto::sui::rpc::v2beta2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, GetServiceInfoRequest,
    GetServiceInfoResponse,
};
use sui_rpc_api::client::AuthInterceptor;
use sui_rpc_api::Client;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use url::Url;

pub struct GrpcClient {
    inner: Client,
}

impl GrpcClient {
    pub fn new(
        url: Url,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, Error> {
        let client = if let (Some(username), password) = (username, password) {
            let auth = AuthInterceptor::basic(username, password);
            Client::new(url.to_string())
                .map_err(Self::convert_tonic_error)?
                .with_auth(auth)
        } else {
            Client::new(url.to_string()).map_err(Self::convert_tonic_error)?
        };

        Ok(Self { inner: client })
    }

    pub async fn get_latest_checkpoint(&self) -> Result<u64, Error> {
        let checkpoint = self
            .inner
            .get_latest_checkpoint()
            .await
            .map_err(Self::convert_tonic_error)?;
        Ok(checkpoint.sequence_number)
    }

    pub async fn get_checkpoint_by_sequence(
        &self,
        sequence_number: u64,
    ) -> Result<CertifiedCheckpointSummary, Error> {
        self.inner
            .get_checkpoint_summary(sequence_number)
            .await
            .map_err(Self::convert_tonic_error)
    }

    pub async fn get_service_info(&self) -> Result<GetServiceInfoResponse, Error> {
        let request = GetServiceInfoRequest {};
        let response = self
            .inner
            .raw_client()
            .get_service_info(request)
            .await
            .map_err(Self::convert_tonic_error)?;
        Ok(response.into_inner())
    }

    pub async fn batch_get_transactions_with_balance_changes(
        &self,
        transaction_digests: Vec<TransactionDigest>,
    ) -> Result<BatchGetTransactionsResponse, Error> {
        let digest_strings: Vec<String> = transaction_digests
            .iter()
            .map(|digest| digest.to_string())
            .collect();

        let request = BatchGetTransactionsRequest {
            digests: digest_strings,
            read_mask: None, // Get all fields including balance changes
        };

        let response = self
            .inner
            .raw_client()
            .batch_get_transactions(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        Ok(response.into_inner())
    }

    fn convert_tonic_error(status: tonic::Status) -> Error {
        match status.code() {
            tonic::Code::NotFound => Error::DataError("Not found".to_string()),
            tonic::Code::InvalidArgument => Error::InvalidInput(status.message().to_string()),
            tonic::Code::Internal => Error::InternalError(anyhow::anyhow!(
                "Internal server error: {}",
                status.message()
            )),
            tonic::Code::Unavailable => Error::DataError("Service unavailable".to_string()),
            tonic::Code::DeadlineExceeded => Error::DataError("Request timeout".to_string()),
            _ => Error::InternalError(anyhow::anyhow!("GRPC error: {}", status.message())),
        }
    }
}
