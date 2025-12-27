// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::{authority::AuthorityState, authority_client::AuthorityAPI};
use async_trait::async_trait;
use consensus_types::block::BlockRef;
use sui_config::genesis::Genesis;
use sui_types::{
    committee::EpochId,
    crypto::AuthorityKeyPair,
    error::{SuiError, SuiErrorKind, SuiResult},
    messages_checkpoint::{
        CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
    },
    messages_consensus::ConsensusPosition,
    messages_grpc::{
        ObjectInfoRequest, ObjectInfoResponse, SubmitTxRequest, SubmitTxResponse, SubmitTxResult,
        SystemStateRequest, TransactionInfoRequest, TransactionInfoResponse,
        ValidatorHealthRequest, ValidatorHealthResponse, WaitForEffectsRequest,
        WaitForEffectsResponse,
    },
    sui_system_state::SuiSystemState,
    transaction::{Transaction, VerifiedTransaction},
};

#[derive(Clone, Copy, Default)]
pub struct LocalAuthorityClientFaultConfig {
    pub fail_before_submit_transaction: bool,
    pub fail_after_vote_transaction: bool,
    pub fail_before_handle_confirmation: bool,
    pub fail_after_handle_confirmation: bool,
    pub overload_retry_after_vote_transaction: Option<Duration>,
}

impl LocalAuthorityClientFaultConfig {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone)]
pub struct LocalAuthorityClient {
    pub state: Arc<AuthorityState>,
    pub fault_config: LocalAuthorityClientFaultConfig,
}

#[async_trait]
impl AuthorityAPI for LocalAuthorityClient {
    async fn submit_transaction(
        &self,
        request: SubmitTxRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<SubmitTxResponse, SuiError> {
        if self.fault_config.fail_before_submit_transaction {
            return Err(SuiError::from("Mock error before submit_transaction"));
        }
        let state = self.state.clone();
        let epoch_store = self.state.load_epoch_store_one_call_per_task();

        let raw_request = request.into_raw()?;
        // TODO(fastpath): handle multiple transactions.
        if raw_request.transactions.len() != 1 {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: format!(
                    "Expected exactly 1 transaction in request, got {}",
                    raw_request.transactions.len()
                ),
            }
            .into());
        }

        let deserialized_transaction = bcs::from_bytes::<Transaction>(&raw_request.transactions[0])
            .map_err(|e| SuiErrorKind::TransactionDeserializationError {
                error: e.to_string(),
            })?;
        let transaction = epoch_store
            // Alias versions can be ignored for test authority client; we don't submit to
            // consensus and no one else is voting.
            .verify_transaction_with_current_aliases(deserialized_transaction.clone())
            .map(|_| VerifiedTransaction::new_from_verified(deserialized_transaction))?;
        state.handle_vote_transaction(&epoch_store, transaction.clone())?;
        if self.fault_config.fail_after_vote_transaction {
            return Err(SuiErrorKind::GenericAuthorityError {
                error: "Mock error after vote transaction in submit_transaction".to_owned(),
            }
            .into());
        }
        if let Some(duration) = self.fault_config.overload_retry_after_vote_transaction {
            return Err(SuiErrorKind::ValidatorOverloadedRetryAfter {
                retry_after_secs: duration.as_secs(),
            }
            .into());
        }

        // No submission to consensus is needed for test authority client, return
        // dummy consensus position
        // TODO(fastpath): Return the actual consensus position
        let consensus_position = ConsensusPosition {
            epoch: EpochId::MIN,
            block: BlockRef::MIN,
            index: 0,
        };

        let submit_result = SubmitTxResult::Submitted { consensus_position };
        Ok(SubmitTxResponse {
            results: vec![submit_result],
        })
    }

    async fn wait_for_effects(
        &self,
        _request: WaitForEffectsRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<WaitForEffectsResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_object_info_request(request).await
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_transaction_info_request(request).await
    }

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let state = self.state.clone();

        state.handle_checkpoint_request(&request)
    }

    async fn handle_checkpoint_v2(
        &self,
        request: CheckpointRequestV2,
    ) -> Result<CheckpointResponseV2, SuiError> {
        let state = self.state.clone();

        state.handle_checkpoint_request_v2(&request)
    }

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        self.state.get_sui_system_state_object_for_testing()
    }

    async fn validator_health(
        &self,
        _request: ValidatorHealthRequest,
    ) -> Result<ValidatorHealthResponse, SuiError> {
        Ok(ValidatorHealthResponse {
            last_committed_leader_round: 1000,
            last_locally_built_checkpoint: 500,
            ..Default::default()
        })
    }
}

impl LocalAuthorityClient {
    pub async fn new(secret: AuthorityKeyPair, genesis: &Genesis) -> Self {
        let state = TestAuthorityBuilder::new()
            .with_genesis_and_keypair(genesis, &secret)
            .build()
            .await;
        Self {
            state,
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }

    pub fn new_from_authority(state: Arc<AuthorityState>) -> Self {
        Self {
            state,
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }
}

// TODO: The way we are passing in and using delay and count is really ugly code. Please fix it.
#[derive(Clone)]
pub struct MockAuthorityApi {
    delay: Duration,
    count: Arc<Mutex<u32>>,
    handle_object_info_request_result: Option<SuiResult<ObjectInfoResponse>>,
}

impl MockAuthorityApi {
    pub fn new(delay: Duration, count: Arc<Mutex<u32>>) -> Self {
        MockAuthorityApi {
            delay,
            count,
            handle_object_info_request_result: None,
        }
    }

    pub fn set_handle_object_info_request(&mut self, result: SuiResult<ObjectInfoResponse>) {
        self.handle_object_info_request_result = Some(result);
    }
}

#[async_trait]
impl AuthorityAPI for MockAuthorityApi {
    /// Submit a new transaction to a Sui or Primary account.
    async fn submit_transaction(
        &self,
        _request: SubmitTxRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<SubmitTxResponse, SuiError> {
        unimplemented!();
    }

    async fn wait_for_effects(
        &self,
        _request: WaitForEffectsRequest,
        _client_addr: Option<SocketAddr>,
    ) -> Result<WaitForEffectsResponse, SuiError> {
        unimplemented!()
    }

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        self.handle_object_info_request_result.clone().unwrap()
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let count = {
            let mut count = self.count.lock().unwrap();
            *count += 1;
            *count
        };

        // timeout until the 15th request
        if count < 15 {
            tokio::time::sleep(self.delay).await;
        }

        Err(SuiErrorKind::TransactionNotFound {
            digest: request.transaction_digest,
        }
        .into())
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!();
    }

    async fn handle_checkpoint_v2(
        &self,
        _request: CheckpointRequestV2,
    ) -> Result<CheckpointResponseV2, SuiError> {
        unimplemented!();
    }

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        unimplemented!();
    }

    async fn validator_health(
        &self,
        _request: ValidatorHealthRequest,
    ) -> Result<ValidatorHealthResponse, SuiError> {
        Ok(ValidatorHealthResponse {
            last_committed_leader_round: 1000,
            last_locally_built_checkpoint: 500,
            ..Default::default()
        })
    }
}
