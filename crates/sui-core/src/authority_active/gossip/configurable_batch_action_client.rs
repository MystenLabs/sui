// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::AuthorityStore;
use crate::authority_aggregator::authority_aggregator_tests::*;
use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use crate::safe_client::SafeClient;
use async_trait::async_trait;
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Once;
use std::{env, fs};
use sui_adapter::genesis;
use sui_types::base_types::*;
use sui_types::batch::{AuthorityBatch, SignedBatch, UpdateItem};
use sui_types::committee::Committee;
use sui_types::crypto::{KeyPair, PublicKeyBytes};
use sui_types::error::SuiError;
use sui_types::messages::{
    AccountInfoRequest, AccountInfoResponse, BatchInfoRequest, BatchInfoResponseItem,
    ConfirmationTransaction, ConsensusTransaction, ObjectInfoRequest, ObjectInfoResponse,
    Transaction, TransactionInfoRequest, TransactionInfoResponse,
};
use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};
use sui_types::object::Object;

static mut SHOULD_FAIL: bool = true;
static FIXER: Once = Once::new();

fn fix() {
    FIXER.call_once(|| unsafe {
        SHOULD_FAIL = false;
    })
}

#[derive(Clone)]
pub struct TestBatch {
    pub digests: Vec<ExecutionDigests>,
}

#[derive(Clone)]
pub enum BatchAction {
    EmitError(),
    EmitUpdateItem(),
}

#[derive(Clone)]
pub enum BatchActionInternal {
    EmitError(),
    EmitUpdateItem(TestBatch),
}

#[derive(Clone)]
pub struct ConfigurableBatchActionClient {
    state: Arc<AuthorityState>,
    pub action_sequence_internal: Vec<BatchActionInternal>,
}

impl ConfigurableBatchActionClient {
    #[cfg(test)]
    pub async fn new(committee: Committee, address: PublicKeyBytes, secret: KeyPair) -> Self {
        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let store = Arc::new(AuthorityStore::open(path.clone(), None));
        let state = AuthorityState::new(
            committee.clone(),
            address,
            Arc::pin(secret),
            store,
            None,
            None,
            None,
            &sui_config::genesis::Genesis::get_default_genesis(),
            &prometheus::Registry::new(),
        )
        .await;

        ConfigurableBatchActionClient {
            state: Arc::new(state),
            action_sequence_internal: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn register_action_sequence(&mut self, actions: Vec<BatchActionInternal>) {
        self.action_sequence_internal = actions;
    }
}

#[async_trait]
impl AuthorityAPI for ConfigurableBatchActionClient {
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_transaction(transaction).await
    }

    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_confirmation_transaction(transaction).await
    }

    async fn handle_consensus_transaction(
        &self,
        _transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_account_info_request(
        &self,
        _request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        Ok(AccountInfoResponse {
            object_ids: vec![],
            owner: Default::default(),
        })
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
        self.state.handle_transaction_info_request(request).await
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        _request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let mut last_batch = AuthorityBatch::initial();
        let actions = &self.action_sequence_internal;
        let secret = self.state.secret.clone();
        let name = self.state.name;
        let mut items: Vec<Result<BatchInfoResponseItem, SuiError>> = Vec::new();
        let mut seq = 0;
        let zero_batch = SignedBatch::new(AuthorityBatch::initial(), &*secret, name);
        items.push(Ok(BatchInfoResponseItem(UpdateItem::Batch(zero_batch))));
        let _ = actions.iter().for_each(|action| {
            match action {
                BatchActionInternal::EmitUpdateItem(test_batch) => {
                    let mut transactions = Vec::new();
                    for digest in test_batch.digests.clone() {
                        transactions.push((seq, digest));
                        // Safe client requires batches arrive first
                        items.push(Ok(BatchInfoResponseItem(UpdateItem::Transaction((
                            seq, digest,
                        )))));
                        seq += 1;
                    }
                    // batch size of 1
                    let new_batch = AuthorityBatch::make_next(&last_batch, &transactions).unwrap();
                    last_batch = new_batch;
                    items.push({
                        let item = SignedBatch::new(last_batch.clone(), &*secret, name);
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(item)))
                    });
                }
                BatchActionInternal::EmitError() => unsafe {
                    if SHOULD_FAIL {
                        fix();
                        items.push(Err(SuiError::GenericAuthorityError {
                            error: "Synthetic authority error".to_string(),
                        }))
                    }
                },
            };
        });

        Ok(Box::pin(tokio_stream::iter(items)))
    }

    async fn handle_checkpoint(
        &self,
        _request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        todo!();
    }
}

#[cfg(test)]
pub async fn init_configurable_authorities(
    authority_action: Vec<BatchAction>,
) -> (
    BTreeMap<AuthorityName, ConfigurableBatchActionClient>,
    Vec<Arc<AuthorityState>>,
    Vec<ExecutionDigests>,
) {
    let authority_count = 4;
    let (addr1, key1) = KeyPair::get_key_pair();
    let mut gas_objects = Vec::new();
    for _i in 0..authority_action.len() {
        gas_objects.push(Object::with_owner_for_testing(addr1));
    }
    let genesis_objects = authority_genesis_objects(authority_count, gas_objects.clone());

    // Create committee.
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for _ in 0..authority_count {
        let (_, key_pair) = KeyPair::get_key_pair();
        let authority_name = *key_pair.public_key_bytes();
        voting_rights.insert(authority_name, 1);
        key_pairs.push((authority_name, key_pair));
    }
    let committee = Committee::new(0, voting_rights).unwrap();

    // Create Authority Clients and States.
    let mut clients = Vec::new();
    let mut names = Vec::new();
    let mut states = Vec::new();
    for ((authority_name, secret), objects) in key_pairs.into_iter().zip(genesis_objects) {
        let client =
            ConfigurableBatchActionClient::new(committee.clone(), authority_name, secret).await;
        for object in objects {
            client.state.insert_genesis_object(object).await;
        }
        states.push(client.state.clone());
        names.push(authority_name);
        clients.push(SafeClient::new(client, committee.clone(), authority_name));
    }

    // Execute transactions for every EmitUpdateItem Action, use the digest of the transaction to
    // create a batch action internal sequence.
    let mut to_be_executed_digests = Vec::new();
    let mut batch_action_internal = Vec::new();
    let framework_obj_ref = genesis::get_framework_object_ref();

    for (action, gas_object) in authority_action.iter().zip(gas_objects) {
        if let BatchAction::EmitUpdateItem() = action {
            let temp_client = clients[0].borrow();
            let gas_ref = get_latest_ref(temp_client, gas_object.id()).await;
            let transaction =
                crate_object_move_transaction(addr1, &key1, addr1, 100, framework_obj_ref, gas_ref);

            // TODO: `take` here only works when each validator has equal stake.
            for tx_client in clients
                .iter_mut()
                .take(committee.quorum_threshold() as usize)
            {
                // Do transactions.
                do_transaction(tx_client, &transaction).await;
            }
            // Add the digest and number to the internal actions.
            let t_b = TestBatch {
                // TODO: need to put in here the real effects digest
                digests: vec![ExecutionDigests::new(
                    *transaction.digest(),
                    TransactionEffectsDigest::random(),
                )],
            };
            batch_action_internal.push(BatchActionInternal::EmitUpdateItem(t_b));
            to_be_executed_digests.push(*transaction.digest());
        }
        if let BatchAction::EmitError() = action {
            batch_action_internal.push(BatchActionInternal::EmitError());
        }
    }

    // Create BtreeMap of names to clients.
    let mut authority_clients = BTreeMap::new();
    for (name, client) in names.into_iter().zip(clients) {
        authority_clients.insert(name, client);
    }

    let mut executed_digests = Vec::new();
    // Execute certificate for each digest, and register the action sequence on the authorities who executed the certificates.
    for digest in to_be_executed_digests.clone() {
        // Get a cert
        let authority_clients_ref: Vec<_> = authority_clients.values().collect();
        let authority_clients_slice = authority_clients_ref.as_slice();
        let cert1 = extract_cert(authority_clients_slice, &committee, &digest).await;

        let mut effects_digest = TransactionEffectsDigest::random();
        // Submit the cert to 2f+1 authorities.
        for (_, cert_client) in authority_clients
            .iter_mut()
            // TODO: This only works when every validator has equal stake
            .take(committee.quorum_threshold() as usize)
        {
            let effects = do_cert(cert_client, &cert1).await;
            effects_digest = effects.digest();

            // Register the internal actions to client
            cert_client
                .authority_client_mut()
                .register_action_sequence(batch_action_internal.clone());
        }
        executed_digests.push(ExecutionDigests::new(digest, effects_digest));
    }

    let authority_clients = authority_clients
        .into_iter()
        .map(|(name, client)| (name, client.authority_client().clone()))
        .collect();
    (authority_clients, states, executed_digests)
}
