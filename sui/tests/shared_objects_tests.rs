// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use sui::config::AuthorityPrivateInfo;
use sui_types::error::SuiError;
use sui_types::messages::CallArg;
use sui_types::messages::Transaction;
use sui_types::messages::TransactionInfoResponse;
use sui_types::messages::{ConsensusTransaction, ExecutionStatus};
use sui_types::serialize::{
    deserialize_message, deserialize_transaction_info, serialize_consensus_transaction,
};
use sui_types::serialize::{serialize_cert, SerializedMessage};
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::{make_certificates, move_transaction, publish_move_package_transaction};
use test_utils::messages::{parse_package_ref, test_shared_object_transactions};
use test_utils::objects::{test_gas_objects, test_shared_object};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::codec::LengthDelimitedCodec;

/// Send bytes to a Sui authority.
async fn transmit(transaction: Bytes, config: &AuthorityPrivateInfo) -> SerializedMessage {
    let authority_address = format!("{}:{}", config.host, config.port);
    let stream = TcpStream::connect(authority_address).await.unwrap();
    let mut connection = Framed::new(stream, LengthDelimitedCodec::new());

    connection.send(transaction).await.unwrap();
    let bytes = connection.next().await.unwrap().unwrap();
    deserialize_message(&bytes[..]).unwrap()
}

/// Submit a certificate containing only owned-objects to all authorities.
async fn submit_single_owner_transaction(
    transaction: Transaction,
    configs: &[AuthorityPrivateInfo],
) -> Vec<TransactionInfoResponse> {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();
    let serialized = Bytes::from(serialize_cert(&certificate));

    let mut responses = Vec::new();
    for config in configs {
        let bytes = transmit(serialized.clone(), config).await;
        let reply = deserialize_transaction_info(bytes).unwrap();
        responses.push(reply);
    }
    responses
}

// Keep submitting the certificate until it is sequenced by consensus. We use the loop
// since some consensus protocols (like Tusk) are not guaranteed to include the transaction
// (but it has high probability to do so, so it should virtually never be used).
async fn submit_shared_object_transaction(
    transaction: Transaction,
    configs: &[AuthorityPrivateInfo],
) -> TransactionInfoResponse {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();
    let message = ConsensusTransaction::UserTransaction(certificate);
    let serialized = Bytes::from(serialize_consensus_transaction(&message));

    'main: loop {
        match transmit(serialized.clone(), &configs[0]).await {
            SerializedMessage::TransactionResp(reply) => {
                // We got a reply from the Sui authority.
                break 'main *reply;
            }
            SerializedMessage::Error(error) => match *error {
                SuiError::ConsensusConnectionBroken(_) => {
                    // This is the (confusing) error message returned by the consensus
                    // adapter. It means it didn't hear back from consensus and timed out.
                }
                error => panic!("{error}"),
            },
            message => panic!("Unexpected protocol message: {message:?}"),
        }
    }
}

#[tokio::test]
#[ignore = "Flaky, see #1624"]
async fn shared_object_transaction() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    tokio::task::yield_now().await;
    let reply = submit_shared_object_transaction(transaction, &configs).await;
    assert!(reply.signed_effects.is_some());
}

#[tokio::test]
#[ignore = "Flaky, see #1624"]
async fn call_shared_object_contract() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    let transaction = publish_move_package_transaction(gas_objects.pop().unwrap());
    let replies = submit_single_owner_transaction(transaction, &configs).await;
    let mut package_refs = Vec::new();
    for reply in replies {
        let effects = reply.signed_effects.unwrap().effects;
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
        package_refs.push(parse_package_ref(&effects).unwrap());
    }
    let package_ref = package_refs.pop().unwrap();

    // Make a transaction to create a counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let replies = submit_single_owner_transaction(transaction, &configs).await;
    let mut counter_ids = Vec::new();
    for reply in replies {
        let effects = reply.signed_effects.unwrap().effects;
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
        let ((shared_object_id, _, _), _) = effects.created[0];
        counter_ids.push(shared_object_id);
    }
    let counter_id = counter_ids.pop().unwrap();

    // Ensure the value of the counter is `0`.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::SharedObject(counter_id),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );
    let reply = submit_shared_object_transaction(transaction, &configs).await;
    let effects = reply.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "increment",
        package_ref,
        vec![CallArg::SharedObject(counter_id)],
    );
    let reply = submit_shared_object_transaction(transaction, &configs).await;
    let effects = reply.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Ensure the value of the counter is `1`.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::SharedObject(counter_id),
            CallArg::Pure(1u64.to_le_bytes().to_vec()),
        ],
    );
    let reply = submit_shared_object_transaction(transaction, &configs).await;
    let effects = reply.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}
