// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::SequencedConsensusTransaction;
use crate::test_utils::{test_certificates, test_gas_objects};
use narwhal_types::Transactions;
use narwhal_types::TransactionsServer;
use narwhal_types::{Empty, TransactionProto};
use sui_network::tonic;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Object;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::{Receiver, Sender};

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
                .process_consensus_transactions_for_tests(
                    vec![SequencedConsensusTransaction::new_test(transaction.clone())],
                    &Arc::new(CheckpointServiceNoop {}),
                    self.0.db(),
                    &self.0.metrics.skipped_consensus_txns,
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
        None,
        None,
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
