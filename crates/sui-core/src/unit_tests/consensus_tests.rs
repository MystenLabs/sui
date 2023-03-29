// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::VerifiedSequencedConsensusTransaction;
use move_core_types::{account_address::AccountAddress, ident_str};
use narwhal_types::Transactions;
use narwhal_types::TransactionsServer;
use narwhal_types::{Empty, TransactionProto};
use sui_network::tonic;
use sui_types::crypto::deterministic_random_account_key;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS;
use sui_types::multiaddr::Multiaddr;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use sui_types::{
    base_types::ObjectID,
    messages::{CallArg, CertifiedTransaction, ObjectArg, TransactionData},
    object::Object,
};
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::{Receiver, Sender};

/// Fixture: a few test gas objects.
pub fn test_gas_objects() -> Vec<Object> {
    thread_local! {
        static GAS_OBJECTS: Vec<Object> = (0..4)
            .map(|_| {
                let gas_object_id = ObjectID::random();
                let (owner, _) = deterministic_random_account_key();
                Object::with_id_owner_for_testing(gas_object_id, owner)
            })
            .collect();
    }

    GAS_OBJECTS.with(|v| v.clone())
}

/// Fixture: a few test certificates containing a shared object.
pub async fn test_certificates(authority: &AuthorityState) -> Vec<CertifiedTransaction> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let (sender, keypair) = deterministic_random_account_key();
    let rgp = epoch_store.reference_gas_price();

    let mut certificates = Vec::new();
    let shared_object = Object::shared_for_testing();
    let shared_object_arg = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: shared_object.version(),
        mutable: true,
    };
    for gas_object in test_gas_objects() {
        // Object digest may be different in genesis than originally generated.
        let gas_object = authority
            .get_object(&gas_object.id())
            .await
            .unwrap()
            .unwrap();
        // Make a sample transaction.
        let module = "object_basics";
        let function = "create";

        let data = TransactionData::new_move_call(
            sender,
            SUI_FRAMEWORK_OBJECT_ID,
            ident_str!(module).to_owned(),
            ident_str!(function).to_owned(),
            /* type_args */ vec![],
            gas_object.compute_object_reference(),
            /* args */
            vec![
                CallArg::Object(shared_object_arg),
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
            rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
            rgp,
        )
        .unwrap();

        let transaction = to_sender_signed_transaction(data, &keypair);

        // Submit the transaction and assemble a certificate.
        let response = authority
            .handle_transaction(&epoch_store, transaction.clone())
            .await
            .unwrap();
        let vote = response.status.into_signed_for_testing();
        let certificate = CertifiedTransaction::new(
            transaction.into_message(),
            &[vote],
            &authority.clone_committee_for_testing(),
        )
        .unwrap();
        certificates.push(certificate);
    }
    certificates
}

#[tokio::test]
async fn submit_transaction_to_consensus_adapter() {
    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test certificate.
    let mut objects = test_gas_objects();
    objects.push(Object::shared_for_testing());
    let state = init_state_with_objects(objects).await;
    let certificate = test_certificates(&state).await.pop().unwrap();

    let metrics = ConsensusAdapterMetrics::new_test();

    #[derive(Clone)]
    struct SubmitDirectly(Arc<AuthorityState>);

    #[async_trait::async_trait]
    impl SubmitToConsensus for SubmitDirectly {
        async fn submit_to_consensus(
            &self,
            transaction: &ConsensusTransaction,
            epoch_store: &Arc<AuthorityPerEpochStore>,
        ) -> SuiResult {
            epoch_store
                .process_consensus_transactions(
                    vec![VerifiedSequencedConsensusTransaction::new_test(
                        transaction.clone(),
                    )],
                    &Arc::new(CheckpointServiceNoop {}),
                    self.0.db(),
                )
                .await?;
            Ok(())
        }
    }
    // Make a new consensus adapter instance.
    let adapter = Arc::new(ConsensusAdapter::new(
        Box::new(SubmitDirectly(state.clone())),
        state.name,
        Box::new(Arc::new(ConnectionMonitorStatusForTests {})),
        100_000,
        100_000,
        metrics,
    ));

    // Submit the transaction and ensure the adapter reports success to the caller. Note
    // that consensus may drop some transactions (so we may need to resubmit them).
    let transaction = ConsensusTransaction::new_certificate_message(&state.name, certificate);
    let epoch_store = state.epoch_store_for_testing();
    let waiter = adapter
        .submit(
            transaction.clone(),
            Some(&epoch_store.get_reconfig_state_read_lock_guard()),
            &epoch_store,
        )
        .unwrap();
    waiter.await.unwrap();
}

pub struct ConsensusMockServer {
    sender: Sender<TransactionProto>,
}

impl ConsensusMockServer {
    pub fn spawn(address: Multiaddr) -> Receiver<TransactionProto> {
        let (sender, receiver) = channel(1);
        tokio::spawn(async move {
            let config = mysten_network::config::Config::new();
            let mock = Self { sender };
            config
                .server_builder()
                .add_service(TransactionsServer::new(mock))
                .bind(&address)
                .await
                .unwrap()
                .serve()
                .await
        });
        receiver
    }
}

#[tonic::async_trait]
impl Transactions for ConsensusMockServer {
    /// Submit a Transactions
    async fn submit_transaction(
        &self,
        request: tonic::Request<TransactionProto>,
    ) -> Result<tonic::Response<Empty>, tonic::Status> {
        self.sender.send(request.into_inner()).await.unwrap();
        Ok(tonic::Response::new(Empty {}))
    }
    /// Submit a Transactions
    async fn submit_transaction_stream(
        &self,
        _request: tonic::Request<tonic::Streaming<TransactionProto>>,
    ) -> Result<tonic::Response<Empty>, tonic::Status> {
        unimplemented!()
    }
}
