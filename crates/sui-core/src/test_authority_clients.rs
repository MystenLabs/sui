// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{authority::AuthorityState, authority_client::AuthorityAPI};
use async_trait::async_trait;
use mysten_metrics::spawn_monitored_task;
use sui_config::genesis::Genesis;
use sui_types::{
    committee::Committee,
    crypto::AuthorityKeyPair,
    error::SuiError,
    messages::{
        CertifiedTransaction, CommitteeInfoRequest, CommitteeInfoResponse, ObjectInfoRequest,
        ObjectInfoResponse, Transaction, TransactionInfoRequest, TransactionInfoResponse,
    },
    messages_checkpoint::{CheckpointRequest, CheckpointResponse},
};
use sui_types::{error::SuiResult, messages::HandleCertificateResponse};

#[derive(Clone, Copy, Default)]
pub struct LocalAuthorityClientFaultConfig {
    pub fail_before_handle_transaction: bool,
    pub fail_after_handle_transaction: bool,
    pub fail_before_handle_confirmation: bool,
    pub fail_after_handle_confirmation: bool,
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
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if self.fault_config.fail_before_handle_transaction {
            return Err(SuiError::from("Mock error before handle_transaction"));
        }
        let state = self.state.clone();
        let transaction = transaction.verify()?;
        let result = state.handle_transaction(transaction).await;
        if self.fault_config.fail_after_handle_transaction {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_transaction".to_owned(),
            });
        }
        result.map(|v| v.into())
    }

    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        let state = self.state.clone();
        let fault_config = self.fault_config;
        spawn_monitored_task!(Self::handle_certificate(state, certificate, fault_config))
            .await
            .unwrap()
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let state = self.state.clone();
        state
            .handle_object_info_request(request)
            .await
            .map(|r| r.into())
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state
            .handle_transaction_info_request(request)
            .await
            .map(|r| r.into())
    }

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let state = self.state.clone();

        state.handle_checkpoint_request(&request)
    }

    async fn handle_committee_info_request(
        &self,
        request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError> {
        let state = self.state.clone();

        state.handle_committee_info_request(&request)
    }
}

impl LocalAuthorityClient {
    pub async fn new(committee: Committee, secret: AuthorityKeyPair, genesis: &Genesis) -> Self {
        let state = AuthorityState::new_for_testing(committee, &secret, None, genesis).await;
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

    async fn handle_certificate(
        state: Arc<AuthorityState>,
        certificate: CertifiedTransaction,
        fault_config: LocalAuthorityClientFaultConfig,
    ) -> Result<HandleCertificateResponse, SuiError> {
        if fault_config.fail_before_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_confirmation_transaction".to_owned(),
            });
        }
        // Check existing effects before verifying the cert to allow querying certs finalized
        // from previous epochs.
        let tx_digest = *certificate.digest();
        let epoch_store = state.epoch_store();
        let signed_effects =
            match state.get_signed_effects_and_maybe_resign(epoch_store.epoch(), &tx_digest) {
                Ok(Some(effects)) => effects,
                _ => {
                    let certificate = { certificate.verify(epoch_store.committee())? };
                    state
                        .try_execute_immediately(&certificate, &epoch_store)
                        .await?
                }
            }
            .into_inner();
        if fault_config.fail_after_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_confirmation_transaction".to_owned(),
            });
        }
        Ok(HandleCertificateResponse { signed_effects })
    }
}

#[derive(Clone)]
pub struct MockAuthorityApi {
    delay: Duration,
    count: Arc<Mutex<u32>>,
    handle_committee_info_request_result: Option<SuiResult<CommitteeInfoResponse>>,
    handle_object_info_request_result: Option<SuiResult<ObjectInfoResponse>>,
}

impl MockAuthorityApi {
    pub fn new(delay: Duration, count: Arc<Mutex<u32>>) -> Self {
        MockAuthorityApi {
            delay,
            count,
            handle_committee_info_request_result: None,
            handle_object_info_request_result: None,
        }
    }
    pub fn set_handle_committee_info_request_result(
        &mut self,
        result: SuiResult<CommitteeInfoResponse>,
    ) {
        self.handle_committee_info_request_result = Some(result);
    }

    pub fn set_handle_object_info_request(&mut self, result: SuiResult<ObjectInfoResponse>) {
        self.handle_object_info_request_result = Some(result);
    }
}

#[async_trait]
impl AuthorityAPI for MockAuthorityApi {
    /// Initiate a new transaction to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        unreachable!();
    }

    /// Execute a certificate.
    async fn handle_certificate(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        unreachable!()
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

        Err(SuiError::TransactionNotFound {
            digest: request.transaction_digest,
        })
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unreachable!();
    }

    async fn handle_committee_info_request(
        &self,
        _request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError> {
        self.handle_committee_info_request_result.clone().unwrap()
    }
}

#[derive(Clone)]
pub struct HandleTransactionTestAuthorityClient {
    pub tx_info_resp_to_return: SuiResult<TransactionInfoResponse>,
}

#[async_trait]
impl AuthorityAPI for HandleTransactionTestAuthorityClient {
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.tx_info_resp_to_return.clone()
    }

    async fn handle_certificate(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_transaction_info_request(
        &self,
        _request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_committee_info_request(
        &self,
        _request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError> {
        unimplemented!()
    }
}

impl HandleTransactionTestAuthorityClient {
    pub fn new() -> Self {
        Self {
            tx_info_resp_to_return: Err(SuiError::Unknown("".to_string())),
        }
    }

    pub fn set_tx_info_response(&mut self, resp: TransactionInfoResponse) {
        self.tx_info_resp_to_return = Ok(resp);
    }

    pub fn reset_tx_info_response(&mut self) {
        self.tx_info_resp_to_return = Err(SuiError::Unknown("".to_string()));
    }
}

impl Default for HandleTransactionTestAuthorityClient {
    fn default() -> Self {
        Self::new()
    }
}
