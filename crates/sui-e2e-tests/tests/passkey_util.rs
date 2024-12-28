// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use passkey_authenticator::UserValidationMethod;
use shared_crypto::intent::IntentMessage;
use std::net::SocketAddr;
use sui_core::authority_client::AuthorityAPI;
use sui_types::base_types::SuiAddress;
use sui_types::error::SuiResult;
use sui_types::transaction::Transaction;
use test_cluster::TestCluster;

/// Helper struct to initialize passkey client.
pub struct MyUserValidationMethod {}
#[async_trait::async_trait]
impl UserValidationMethod for MyUserValidationMethod {
    async fn check_user_presence(&self) -> bool {
        true
    }

    async fn check_user_verification(&self) -> bool {
        true
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(true)
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }
}

/// Response with fields from passkey authentication.
#[derive(Debug)]
pub struct PasskeyResponse<T> {
    pub user_sig_bytes: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub client_data_json: String,
    pub intent_msg: IntentMessage<T>,
    pub sender: SuiAddress,
}

/// Submits a transaction to the test cluster and returns the result.
pub async fn execute_tx(tx: Transaction, test_cluster: &TestCluster) -> SuiResult {
    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(tx, Some(SocketAddr::new([127, 0, 0, 1].into(), 0)))
        .await
        .map(|_| ())
}
