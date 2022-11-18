// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::{authority_tests::init_state_with_objects, AuthorityState};
use crate::consensus_handler::VerifiedSequencedConsensusTransaction;
use crate::test_utils::to_sender_signed_transaction;
use move_core_types::{account_address::AccountAddress, ident_str};
use multiaddr::Multiaddr;
use narwhal_types::TransactionsServer;
use narwhal_types::{Certificate, ConsensusOutput, Transactions};
use narwhal_types::{Empty, TransactionProto};
use sui_network::tonic;
use sui_types::{
    base_types::{ObjectID, TransactionDigest},
    gas_coin::GasCoin,
    messages::{CallArg, CertifiedTransaction, ObjectArg, TransactionData},
    object::{MoveObject, Object, Owner, OBJECT_START_VERSION},
};
use test_utils::test_account_keys;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::{Receiver, Sender};

/// Fixture: a few test gas objects.
pub fn test_gas_objects() -> Vec<Object> {
    (0..4)
        .map(|i| {
            let seed = format!("0x555555555555555{i}");
            let gas_object_id = ObjectID::from_hex_literal(&seed).unwrap();
            let (sender, _) = test_account_keys().pop().unwrap();
            Object::with_id_owner_for_testing(gas_object_id, sender)
        })
        .collect()
}

/// Fixture: a a test shared object.
pub fn test_shared_object() -> Object {
    let seed = "0x6666666666666660";
    let shared_object_id = ObjectID::from_hex_literal(seed).unwrap();
    let content = GasCoin::new(shared_object_id, 10);
    let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, content.to_bcs_bytes());
    let owner = Owner::Shared {
        initial_shared_version: obj.version(),
    };
    Object::new_move(obj, owner, TransactionDigest::genesis())
}

/// Fixture: a few test certificates containing a shared object.
pub async fn test_certificates(authority: &AuthorityState) -> Vec<CertifiedTransaction> {
    let (sender, keypair) = test_account_keys().pop().unwrap();

    let mut certificates = Vec::new();
    let shared_object = test_shared_object();
    let shared_object_arg = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: shared_object.version(),
    };
    for gas_object in test_gas_objects() {
        // Make a sample transaction.
        let module = "object_basics";
        let function = "create";
        let package_object_ref = authority.get_framework_object_ref().await.unwrap();

        let data = TransactionData::new_move_call(
            sender,
            package_object_ref,
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
            /* max_gas */ 10_000,
        );

        let transaction = to_sender_signed_transaction(data, &keypair);

        // Submit the transaction and assemble a certificate.
        let response = authority
            .handle_transaction(transaction.clone())
            .await
            .unwrap();
        let vote = response.signed_transaction.unwrap();
        let certificate = CertifiedTransaction::new(
            transaction.into_message(),
            vec![vote.auth_sig().clone()],
            &authority.committee.load(),
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
    objects.push(test_shared_object());
    let state = init_state_with_objects(objects).await;
    let certificate = test_certificates(&state).await.pop().unwrap();

    let state = Arc::new(state);
    let metrics = ConsensusAdapterMetrics::new_test();

    #[derive(Clone)]
    struct SubmitDirectly(Arc<AuthorityState>);

    #[async_trait::async_trait]
    impl SubmitToConsensus for SubmitDirectly {
        async fn submit_to_consensus(&self, transaction: &ConsensusTransaction) -> SuiResult {
            let authority = self.0.name;
            let certificate = Certificate::new_test_empty(authority.try_into().unwrap());
            let output = ConsensusOutput {
                certificate,
                ..Default::default()
            };
            self.0
                .handle_consensus_transaction(
                    &output,
                    VerifiedSequencedConsensusTransaction::new_test(transaction.clone()),
                )
                .await
        }
    }
    // Make a new consensus adapter instance.
    let adapter = ConsensusAdapter::new(
        Box::new(SubmitDirectly(state.clone())),
        state.clone(),
        metrics,
    );

    // Submit the transaction and ensure the adapter reports success to the caller. Note
    // that consensus may drop some transactions (so we may need to resubmit them).
    let transaction = ConsensusTransaction::new_certificate_message(&state.name, certificate);
    loop {
        match adapter.submit(transaction.clone()).await {
            Ok(_) => break,
            Err(SuiError::ConsensusConnectionBroken(..)) => (),
            Err(e) => panic!("Unexpected error message: {e}"),
        }
    }
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
