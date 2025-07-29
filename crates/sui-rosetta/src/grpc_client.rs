// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use bytes::Bytes;
use std::collections::HashMap;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, Checkpoint as ProtoCheckpoint,
    Epoch, GetBalanceRequest, GetCheckpointRequest, GetEpochRequest, GetServiceInfoRequest,
    GetServiceInfoResponse, ListOwnedObjectsRequest, ListOwnedObjectsResponse,
};
use sui_rpc_api::client::AuthInterceptor;
use sui_rpc_api::Client;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::governance::StakedSui;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointDigest};
use url::Url;

#[derive(Clone)]
pub struct GrpcClient {
    inner: Client,
}

pub struct ValidatorInfo {
    pub address: SuiAddress,
    pub staking_pool_id: ObjectID,
    pub exchange_rate: f64,
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

    pub async fn get_checkpoint_with_transactions_by_sequence(
        &self,
        sequence_number: u64,
    ) -> Result<ProtoCheckpoint, Error> {
        let request = GetCheckpointRequest {
            checkpoint_id: Some(sui_rpc::proto::sui::rpc::v2beta2::get_checkpoint_request::CheckpointId::SequenceNumber(sequence_number)),
            read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                "summary",
                "summary.timestamp",
                "summary.previous_digest",
                "contents", 
                "transactions.transaction",
                "transactions.transaction.bcs",
                "transactions.effects",
                "transactions.effects.status",
                "transactions.effects.gas_used",
                "transactions.effects.gas_object",
                "transactions.balance_changes",
            ])),
        };

        let response = self
            .inner
            .raw_client()
            .get_checkpoint(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        response
            .into_inner()
            .checkpoint
            .ok_or_else(|| Error::DataError("No checkpoint returned".to_string()))
    }

    pub async fn get_checkpoint_with_transactions_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> Result<ProtoCheckpoint, Error> {
        let request = GetCheckpointRequest {
            checkpoint_id: Some(
                sui_rpc::proto::sui::rpc::v2beta2::get_checkpoint_request::CheckpointId::Digest(
                    digest.to_string(),
                ),
            ),
            read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                "summary",
                "summary.timestamp",
                "summary.previous_digest",
                "contents",
                "transactions.transaction",
                "transactions.transaction.bcs",
                "transactions.effects",
                "transactions.effects.status",
                "transactions.effects.gas_used",
                "transactions.effects.gas_object",
                "transactions.balance_changes",
            ])),
        };

        let response = self
            .inner
            .raw_client()
            .get_checkpoint(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        response
            .into_inner()
            .checkpoint
            .ok_or_else(|| Error::DataError("No checkpoint returned".to_string()))
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

    pub async fn get_epoch(&self, epoch: Option<u64>) -> Result<Epoch, Error> {
        let request = GetEpochRequest {
            epoch,
            read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                "epoch",
                "system_state",
                "system_state.validators",
                "system_state.validators.active_validators",
                "system_state.validators.staking_pool_mappings",
            ])),
        };

        let response = self
            .inner
            .raw_client()
            .get_epoch(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        response
            .into_inner()
            .epoch
            .ok_or_else(|| Error::DataError("No epoch returned".to_string()))
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

    pub async fn get_balance(&self, owner: SuiAddress, coin_type: String) -> Result<u64, Error> {
        let request = GetBalanceRequest {
            owner: Some(owner.to_string()),
            coin_type: Some(coin_type),
        };

        let response = self
            .inner
            .live_data_client()
            .get_balance(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        let balance = response
            .into_inner()
            .balance
            .ok_or_else(|| Error::DataError("No balance returned".to_string()))?
            .balance
            .ok_or_else(|| Error::DataError("No balance value returned".to_string()))?;

        Ok(balance)
    }

    pub async fn list_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<String>,
        cursor: Option<Bytes>,
    ) -> Result<ListOwnedObjectsResponse, Error> {
        let request = ListOwnedObjectsRequest {
            owner: Some(owner.to_string()),
            object_type,
            page_token: cursor,
            page_size: Some(50), // Default page size
            read_mask: None,     // Get all fields
        };

        let response = self
            .inner
            .live_data_client()
            .list_owned_objects(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        Ok(response.into_inner())
    }

    pub fn extract_validator_info_from_epoch(
        &self,
        epoch: &Epoch,
    ) -> Result<HashMap<ObjectID, ValidatorInfo>, Error> {
        let mut validator_map = HashMap::new();

        let system_state = epoch
            .system_state
            .as_ref()
            .ok_or_else(|| Error::DataError("Missing system state in epoch".to_string()))?;

        let validators = system_state
            .validators
            .as_ref()
            .ok_or_else(|| Error::DataError("Missing validators in system state".to_string()))?;

        // Process active validators
        for validator in &validators.active_validators {
            let address = validator
                .address
                .as_ref()
                .ok_or_else(|| Error::DataError("Missing validator address".to_string()))?
                .parse()
                .map_err(|e| Error::DataError(format!("Invalid validator address: {}", e)))?;

            let staking_pool = validator
                .staking_pool
                .as_ref()
                .ok_or_else(|| Error::DataError("Missing staking pool".to_string()))?;

            let pool_id = staking_pool
                .id
                .as_ref()
                .ok_or_else(|| Error::DataError("Missing staking pool ID".to_string()))?
                .parse()
                .map_err(|e| Error::DataError(format!("Invalid staking pool ID: {}", e)))?;

            // Calculate exchange rate
            let sui_balance = staking_pool.sui_balance.unwrap_or(0) as f64;
            let pool_token_balance = staking_pool.pool_token_balance.unwrap_or(1) as f64;
            let exchange_rate = if pool_token_balance > 0.0 {
                sui_balance / pool_token_balance
            } else {
                1.0
            };

            validator_map.insert(
                pool_id,
                ValidatorInfo {
                    address,
                    staking_pool_id: pool_id,
                    exchange_rate,
                },
            );
        }

        Ok(validator_map)
    }

    pub fn parse_staked_sui_from_proto(
        &self,
        object: &sui_rpc::proto::sui::rpc::v2beta2::Object,
        validator_map: &HashMap<ObjectID, ValidatorInfo>,
    ) -> Result<Option<(StakedSui, ObjectID, SuiAddress)>, Error> {
        // Check if this is a StakedSui object
        let object_type = object
            .object_type
            .as_ref()
            .ok_or_else(|| Error::DataError("Missing object_type in proto object".to_string()))?;

        if object_type != "0x3::staking_pool::StakedSui" {
            return Ok(None);
        }

        // Get object ID
        let object_id = object
            .object_id
            .as_ref()
            .ok_or_else(|| Error::DataError("Missing object_id".to_string()))?
            .parse()
            .map_err(|e| Error::DataError(format!("Invalid object_id: {}", e)))?;

        // Get BCS data
        let bcs_data = object
            .bcs
            .as_ref()
            .ok_or_else(|| Error::DataError("Missing BCS data for StakedSui object".to_string()))?;

        let bcs_bytes = bcs_data.value.as_ref().ok_or_else(|| {
            Error::DataError("Missing BCS value for StakedSui object".to_string())
        })?;

        // Deserialize StakedSui
        let staked_sui: StakedSui = bcs::from_bytes(bcs_bytes)
            .map_err(|e| Error::DataError(format!("Failed to deserialize StakedSui: {}", e)))?;

        // Extract pool_id from StakedSui to determine validator
        let pool_id = staked_sui.pool_id();

        // Look up the validator address from the pool_id
        let validator_info = validator_map.get(&pool_id).ok_or_else(|| {
            Error::DataError(format!("Validator not found for pool ID: {}", pool_id))
        })?;

        Ok(Some((staked_sui, object_id, validator_info.address)))
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
