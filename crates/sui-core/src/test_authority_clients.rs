// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::{authority::AuthorityState, authority_client::AuthorityAPI};
use async_trait::async_trait;
use mysten_metrics::spawn_monitored_task;
use sui_config::genesis::Genesis;
use sui_types::effects::{TransactionEffectsAPI, TransactionEvents};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{
    crypto::AuthorityKeyPair,
    error::SuiError,
    messages::{
        CertifiedTransaction, HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse,
        SystemStateRequest, Transaction, TransactionInfoRequest, TransactionInfoResponse,
    },
    messages_checkpoint::{CheckpointRequest, CheckpointResponse},
};
use sui_types::{
    error::SuiResult,
    messages::{HandleCertificateResponse, HandleCertificateResponseV2},
};

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
    ) -> Result<HandleTransactionResponse, SuiError> {
        if self.fault_config.fail_before_handle_transaction {
            return Err(SuiError::from("Mock error before handle_transaction"));
        }
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let state = self.state.clone();
        let transaction = transaction.verify()?;
        let result = state.handle_transaction(&epoch_store, transaction).await;
        if self.fault_config.fail_after_handle_transaction {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_transaction".to_owned(),
            });
        }
        result
    }

    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        self.handle_certificate_v2(certificate)
            .await
            .map(|r| r.into())
    }

    async fn handle_certificate_v2(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
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

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        Ok(self.state.database.get_sui_system_state_object()?)
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

    async fn handle_certificate(
        state: Arc<AuthorityState>,
        certificate: CertifiedTransaction,
        fault_config: LocalAuthorityClientFaultConfig,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
        if fault_config.fail_before_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_confirmation_transaction".to_owned(),
            });
        }
        // Check existing effects before verifying the cert to allow querying certs finalized
        // from previous epochs.
        let tx_digest = *certificate.digest();
        let epoch_store = state.epoch_store_for_testing();
        let signed_effects =
            match state.get_signed_effects_and_maybe_resign(&tx_digest, &epoch_store) {
                Ok(Some(effects)) => effects,
                _ => {
                    let certificate = certificate.verify(epoch_store.committee())?;
                    state.try_execute_for_test(&certificate).await?.0
                }
            }
            .into_inner();

        let events = if let Some(digest) = signed_effects.events_digest() {
            state.get_transaction_events(digest)?
        } else {
            TransactionEvents::default()
        };

        if fault_config.fail_after_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_confirmation_transaction".to_owned(),
            });
        }
        let fastpath_input_objects = state.load_fastpath_input_objects(&signed_effects)?;
        Ok(HandleCertificateResponseV2 {
            signed_effects,
            events,
            fastpath_input_objects,
        })
    }
}

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
    /// Initiate a new transaction to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<HandleTransactionResponse, SuiError> {
        unimplemented!();
    }

    /// Execute a certificate.
    async fn handle_certificate(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        unimplemented!()
    }

    /// Execute a certificate.
    async fn handle_certificate_v2(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
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

        Err(SuiError::TransactionNotFound {
            digest: request.transaction_digest,
        })
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        unimplemented!();
    }

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        unimplemented!();
    }
}

#[derive(Clone)]
pub struct HandleTransactionTestAuthorityClient {
    pub tx_info_resp_to_return: SuiResult<HandleTransactionResponse>,
}

#[async_trait]
impl AuthorityAPI for HandleTransactionTestAuthorityClient {
    async fn handle_transaction(
        &self,
        _transaction: Transaction,
    ) -> Result<HandleTransactionResponse, SuiError> {
        self.tx_info_resp_to_return.clone()
    }

    async fn handle_certificate(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponse, SuiError> {
        unimplemented!()
    }

    async fn handle_certificate_v2(
        &self,
        _certificate: CertifiedTransaction,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
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

    async fn handle_system_state_object(
        &self,
        _request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        unimplemented!()
    }
}

impl HandleTransactionTestAuthorityClient {
    pub fn new() -> Self {
        Self {
            tx_info_resp_to_return: Err(SuiError::Unknown("".to_string())),
        }
    }

    pub fn set_tx_info_response(&mut self, resp: HandleTransactionResponse) {
        self.tx_info_resp_to_return = Ok(resp);
    }

    pub fn set_tx_info_response_error(&mut self, error: SuiError) {
        self.tx_info_resp_to_return = Err(error);
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
