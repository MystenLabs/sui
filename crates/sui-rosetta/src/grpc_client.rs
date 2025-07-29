// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Error;
use bytes::Bytes;
use std::collections::HashMap;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse,
    Checkpoint as ProtoCheckpoint, Epoch, GetBalanceRequest,
    GetCheckpointRequest, GetCoinInfoRequest, GetCoinInfoResponse, GetEpochRequest,
    GetServiceInfoRequest, GetServiceInfoResponse, ListOwnedObjectsRequest,
    ListOwnedObjectsResponse, SimulateTransactionRequest, SimulateTransactionResponse,
};
use sui_rpc_api::client::{AuthInterceptor, TransactionExecutionResponse};
use sui_rpc_api::Client;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::governance::StakedSui;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointDigest};
use sui_types::transaction::{Transaction, TransactionData};
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

    pub async fn get_transaction_with_details(
        &self,
        digest: TransactionDigest,
    ) -> Result<sui_rpc::proto::sui::rpc::v2beta2::ExecutedTransaction, Error> {
        let request = BatchGetTransactionsRequest {
            digests: vec![digest.to_string()],
            read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                "transaction",
                "transaction.bcs",
                "effects",
                "effects.status",
                "effects.gas_used",
                "effects.gas_object",
                "balance_changes",
                "events",
            ])),
        };

        let response = self
            .inner
            .raw_client()
            .batch_get_transactions(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        let response = response.into_inner();
        let transactions = response.transactions;
        if transactions.is_empty() {
            return Err(Error::DataError(format!(
                "Transaction not found: {}",
                digest
            )));
        }

        let result = transactions.into_iter().next().unwrap();
        match result.result {
            Some(sui_rpc::proto::sui::rpc::v2beta2::get_transaction_result::Result::Transaction(tx)) => Ok(tx),
            Some(sui_rpc::proto::sui::rpc::v2beta2::get_transaction_result::Result::Error(e)) => {
                Err(Error::DataError(format!("Transaction error: {:?}", e)))
            }
            None => Err(Error::DataError("Empty transaction result".to_string())),
        }
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

    pub async fn simulate_transaction(
        &self,
        tx_data: TransactionData,
    ) -> Result<SimulateTransactionResponse, Error> {
        let request = SimulateTransactionRequest {
            transaction: Some(tx_data.into()),
            read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                "transaction",
                "transaction.effects",
                "transaction.effects.status",
                "transaction.effects.gas_used",
                "transaction.effects.gas_used.computation_cost",
                "transaction.effects.gas_used.storage_cost",
                "transaction.effects.gas_used.storage_rebate",
            ])),
            checks: None,
            do_gas_selection: Some(false),
        };

        let response = self
            .inner
            .live_data_client()
            .simulate_transaction(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        Ok(response.into_inner())
    }

    pub async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> Result<TransactionExecutionResponse, Error> {
        let response = self
            .inner
            .execute_transaction(&tx)
            .await
            .map_err(Self::convert_tonic_error)?;

        Ok(response)
    }

    pub async fn get_coin_info(
        &self,
        coin_type: String,
    ) -> Result<GetCoinInfoResponse, Error> {
        let request = GetCoinInfoRequest {
            coin_type: Some(coin_type),
        };

        let response = self
            .inner
            .live_data_client()
            .get_coin_info(request)
            .await
            .map_err(Self::convert_tonic_error)?;

        Ok(response.into_inner())
    }

    pub async fn get_coins_for_address(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        cursor: Option<Bytes>,
    ) -> Result<ListOwnedObjectsResponse, Error> {
        // List owned objects filtered by coin type
        let object_type = coin_type.map(|ct| format!("0x2::coin::Coin<{}>", ct));
        self.list_owned_objects(owner, object_type, cursor).await
    }

    pub async fn get_all_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
    ) -> Result<Vec<(ObjectID, u64, ObjectRef)>, Error> {
        let mut all_coins = Vec::new();
        let mut cursor = None;
        
        loop {
            let response = self.get_coins_for_address(owner, coin_type.clone(), cursor).await?;
            
            for obj in &response.objects {
                let object_id = obj.object_id
                    .as_ref()
                    .ok_or_else(|| Error::DataError("Missing object ID".to_string()))?
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid object ID: {}", e)))?;
                
                // Extract coin balance from object
                if let Some(contents) = &obj.contents {
                    if let Some(value) = &contents.value {
                            // Deserialize coin to get balance
                            let coin: sui_types::coin::Coin = bcs::from_bytes(value)
                                .map_err(|e| Error::DataError(format!("Failed to deserialize coin: {}", e)))?;
                            
                            let balance = coin.balance.value();
                            let version = obj.version
                                .ok_or_else(|| Error::DataError("Missing object version".to_string()))?;
                            let object_ref = (
                                object_id,
                                SequenceNumber::from(version),
                                obj.digest
                                    .as_ref()
                                    .ok_or_else(|| Error::DataError("Missing object digest".to_string()))?
                                    .parse()
                                    .map_err(|e| Error::DataError(format!("Invalid object digest: {}", e)))?,
                            );
                            
                            all_coins.push((object_id, balance, object_ref));
                    }
                }
            }
            
            cursor = response.next_page_token;
            if cursor.is_none() {
                break;
            }
        }
        
        Ok(all_coins)
    }

    pub async fn select_coins(
        &self,
        owner: SuiAddress,
        coin_type: Option<String>,
        amount: u64,
        exclude: Vec<ObjectID>,
    ) -> Result<Vec<(ObjectID, u64, ObjectRef)>, Error> {
        let mut selected_coins = Vec::new();
        let mut total_balance = 0u64;
        let mut cursor = None;
        
        // Keep fetching coins until we have enough
        while total_balance < amount {
            let response = self.get_coins_for_address(owner, coin_type.clone(), cursor).await?;
            
            for obj in &response.objects {
                let object_id = obj.object_id
                    .as_ref()
                    .ok_or_else(|| Error::DataError("Missing object ID".to_string()))?
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid object ID: {}", e)))?;
                
                // Skip if in exclude list
                if exclude.contains(&object_id) {
                    continue;
                }
                
                // Extract coin balance from object
                if let Some(contents) = &obj.contents {
                    if let Some(value) = &contents.value {
                            // Deserialize coin to get balance
                            let coin: sui_types::coin::Coin = bcs::from_bytes(value)
                                .map_err(|e| Error::DataError(format!("Failed to deserialize coin: {}", e)))?;
                            
                            let balance = coin.balance.value();
                            let version = obj.version
                                .ok_or_else(|| Error::DataError("Missing object version".to_string()))?;
                            let object_ref = (
                                object_id,
                                SequenceNumber::from(version),
                                obj.digest
                                    .as_ref()
                                    .ok_or_else(|| Error::DataError("Missing object digest".to_string()))?
                                    .parse()
                                    .map_err(|e| Error::DataError(format!("Invalid object digest: {}", e)))?,
                            );
                            
                            selected_coins.push((object_id, balance, object_ref));
                            total_balance += balance;
                            
                            if total_balance >= amount {
                                return Ok(selected_coins);
                            }
                    }
                }
            }
            
            // Check if there are more pages
            cursor = response.next_page_token;
            if cursor.is_none() {
                break;
            }
        }
        
        if total_balance < amount {
            return Err(Error::InvalidInput(format!(
                "Insufficient balance. Required: {}, available: {}",
                amount, total_balance
            )));
        }
        
        Ok(selected_coins)
    }

    pub async fn get_stakes(
        &self,
        owner: SuiAddress,
    ) -> Result<Vec<ObjectID>, Error> {
        let mut stake_ids = Vec::new();
        let mut cursor = None;
        
        // Fetch all StakedSui objects
        let object_type = Some("0x3::staking_pool::StakedSui".to_string());
        
        loop {
            let response = self.list_owned_objects(owner, object_type.clone(), cursor).await?;
            
            for obj in &response.objects {
                if let Some(object_id) = &obj.object_id {
                    let id = object_id.parse()
                        .map_err(|e| Error::DataError(format!("Invalid object ID: {}", e)))?;
                    stake_ids.push(id);
                }
            }
            
            cursor = response.next_page_token;
            if cursor.is_none() {
                break;
            }
        }
        
        Ok(stake_ids)
    }
    
    pub async fn get_stakes_with_details(
        &self,
        owner: SuiAddress,
    ) -> Result<Vec<(ObjectID, StakedSui, SuiAddress)>, Error> {
        let mut stakes = Vec::new();
        let mut cursor = None;
        
        // First get current epoch to get validator info
        let epoch = self.get_epoch(None).await?;
        let validator_map = self.extract_validator_info_from_epoch(&epoch)?;
        
        // Fetch all StakedSui objects with full details
        let object_type = Some("0x3::staking_pool::StakedSui".to_string());
        
        loop {
            let request = ListOwnedObjectsRequest {
                owner: Some(owner.to_string()),
                object_type: object_type.clone(),
                page_token: cursor,
                page_size: Some(50),
                read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                    "object_id",
                    "version",
                    "digest",
                    "object_type",
                    "bcs",
                ])),
            };
            
            let response = self
                .inner
                .live_data_client()
                .list_owned_objects(request)
                .await
                .map_err(Self::convert_tonic_error)?
                .into_inner();
            
            for obj in &response.objects {
                if let Some((staked_sui, object_id, validator_addr)) = 
                    self.parse_staked_sui_from_proto(obj, &validator_map)? {
                    stakes.push((object_id, staked_sui, validator_addr));
                }
            }
            
            cursor = response.next_page_token;
            if cursor.is_none() {
                break;
            }
        }
        
        Ok(stakes)
    }

    pub async fn get_object_refs(
        &self,
        object_ids: Vec<ObjectID>,
    ) -> Result<Vec<ObjectRef>, Error> {
        // Convert ObjectIDs to strings for the request
        let object_ids_str: Vec<String> = object_ids.iter().map(|id| id.to_string()).collect();
        
        let requests = object_ids_str.into_iter().map(|id| {
            sui_rpc::proto::sui::rpc::v2beta2::GetObjectRequest {
                object_id: Some(id),
                version: None, // Get latest version
                read_mask: Some(sui_rpc::field::FieldMask::from_paths([
                    "object_id",
                    "version", 
                    "digest",
                ])),
            }
        }).collect();
        
        let request = sui_rpc::proto::sui::rpc::v2beta2::BatchGetObjectsRequest {
            requests,
            read_mask: None,
        };
        
        let response = self
            .inner
            .raw_client()
            .batch_get_objects(request)
            .await
            .map_err(Self::convert_tonic_error)?;
        
        let mut object_refs = Vec::new();
        for result in response.into_inner().objects {
            if let Some(sui_rpc::proto::sui::rpc::v2beta2::get_object_result::Result::Object(obj)) = result.result {
                let object_id = obj.object_id
                    .as_ref()
                    .ok_or_else(|| Error::DataError("Missing object ID".to_string()))?
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid object ID: {}", e)))?;
                
                let version = obj.version
                    .ok_or_else(|| Error::DataError("Missing object version".to_string()))?;
                
                let digest = obj.digest
                    .as_ref()
                    .ok_or_else(|| Error::DataError("Missing object digest".to_string()))?
                    .parse()
                    .map_err(|e| Error::DataError(format!("Invalid object digest: {}", e)))?;
                
                object_refs.push((object_id, SequenceNumber::from(version), digest));
            }
        }
        
        Ok(object_refs)
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
