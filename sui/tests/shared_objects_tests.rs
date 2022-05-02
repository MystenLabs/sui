// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use sui::config::AuthorityPrivateInfo;
use sui_types::base_types::ObjectRef;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::CallArg;
use sui_types::messages::Transaction;
use sui_types::messages::TransactionInfoResponse;
use sui_types::messages::{ConsensusTransaction, ExecutionStatus};
use sui_types::object::Object;
use sui_types::serialize::{
    deserialize_message, deserialize_transaction_info, serialize_cert,
    serialize_consensus_transaction, SerializedMessage,
};
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

/// Keep submitting the certificates of a shared-object transaction until it is sequenced by
/// at least one consensus node. We use the loop since some consensus protocols (like Tusk)
/// may drop transactions. The certificate is submitted to every Sui authority.
async fn submit_shared_object_transaction(
    transaction: Transaction,
    configs: &[AuthorityPrivateInfo],
) -> Vec<SuiResult<TransactionInfoResponse>> {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();
    let message = ConsensusTransaction::UserTransaction(certificate);
    let serialized = Bytes::from(serialize_consensus_transaction(&message));

    loop {
        let futures: Vec<_> = configs
            .iter()
            .map(|config| transmit(serialized.clone(), config))
            .collect();

        let mut replies = Vec::new();
        for result in futures::future::join_all(futures).await {
            match result {
                SerializedMessage::TransactionResp(reply) => {
                    // We got a reply from the Sui authority.
                    replies.push(Some(Ok(*reply)));
                }
                SerializedMessage::Error(error) => match *error {
                    SuiError::ConsensusConnectionBroken(_) => {
                        // This is the (confusing, #1489) error message returned by the consensus
                        // adapter. It means it didn't hear back from consensus and timed out.
                        replies.push(None);
                    }
                    error => replies.push(Some(Err(error))),
                },
                message => panic!("Unexpected protocol message: {message:?}"),
            }
        }
        if replies.iter().any(|x| x.is_some()) {
            // Remove all `ConsensusConnectionBroken` replies.
            break replies.into_iter().flatten().collect();
        }
    }
}

/// Helper function to publish the move package of a simple shared counter.
async fn publish_counter_package(
    gas_object: Object,
    configs: &[AuthorityPrivateInfo],
) -> ObjectRef {
    let transaction = publish_move_package_transaction(gas_object);
    let replies = submit_single_owner_transaction(transaction, configs).await;
    let mut package_refs = Vec::new();
    for reply in replies {
        let effects = reply.signed_effects.unwrap().effects;
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
        package_refs.push(parse_package_ref(&effects).unwrap());
    }
    package_refs.pop().unwrap()
}

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[tokio::test]
async fn shared_object_transaction() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, key_pairs) = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs, &key_pairs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    tokio::task::yield_now().await;
    let reply = submit_shared_object_transaction(transaction, &configs[0..1])
        .await
        .pop()
        .unwrap();
    let info = reply.unwrap();
    assert!(info.signed_effects.is_some());
}

/// Same as `shared_object_transaction` but every authorities submit the transaction.
#[tokio::test]
async fn many_shared_object_transactions() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, key_pairs) = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs, &key_pairs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    tokio::task::yield_now().await;
    let replies = submit_shared_object_transaction(transaction, &configs).await;
    for reply in replies {
        match reply {
            Ok(_) => (),
            Err(error) => panic!("{error}"),
        }
    }
}

/// End-to-end shared transaction test for a Sui validator. It does not test the client, wallet,
/// or gateway but tests the end-to-end flow from Sui to consensus.
#[tokio::test]
async fn call_shared_object_contract() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, key_pairs) = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs, &key_pairs).await;

    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    let package_ref = publish_counter_package(gas_objects.pop().unwrap(), &configs).await;

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
    let reply = submit_shared_object_transaction(transaction, &configs[0..1])
        .await
        .pop()
        .unwrap();
    let info = reply.unwrap();
    let effects = info.signed_effects.unwrap().effects;
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
    let reply = submit_shared_object_transaction(transaction, &configs[0..1])
        .await
        .pop()
        .unwrap();
    let info = reply.unwrap();
    let effects = info.signed_effects.unwrap().effects;
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
    let reply = submit_shared_object_transaction(transaction, &configs[0..1])
        .await
        .pop()
        .unwrap();
    let info = reply.unwrap();
    let effects = info.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}

/// Same test as `call_shared_object_contract` but the clients submits many times the same
/// transaction (one copy per authority).
#[tokio::test]
async fn shared_object_flood() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, key_pairs) = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs, &key_pairs).await;

    // Publish the move package to all authorities and get the new pacakge ref.
    tokio::task::yield_now().await;
    let package_ref = publish_counter_package(gas_objects.pop().unwrap(), &configs).await;

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
    let replies = submit_shared_object_transaction(transaction, &configs).await;
    for reply in replies {
        match reply {
            Ok(info) => {
                let effects = info.signed_effects.unwrap().effects;
                assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            }
            Err(error) => panic!("{error}"),
        }
    }

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "increment",
        package_ref,
        vec![CallArg::SharedObject(counter_id)],
    );
    let replies = submit_shared_object_transaction(transaction, &configs).await;
    for reply in replies {
        match reply {
            Ok(info) => {
                let effects = info.signed_effects.unwrap().effects;
                assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            }
            Err(error) => panic!("{error}"),
        }
    }

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
    let replies = submit_shared_object_transaction(transaction, &configs).await;
    for reply in replies {
        match reply {
            Ok(info) => {
                let effects = info.signed_effects.unwrap().effects;
                assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            }
            Err(error) => panic!("{error}"),
        }
    }
}

#[tokio::test]
async fn shared_object_sync() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, key_pairs) = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs, &key_pairs).await;

    // Publish the move package to all authorities and get the new pacakge ref.
    tokio::task::yield_now().await;
    let package_ref = publish_counter_package(gas_objects.pop().unwrap(), &configs).await;

    // Send a transaction to create a counter, but only to one authority.
    tokio::task::yield_now().await;
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let mut replies =
        submit_single_owner_transaction(create_counter_transaction.clone(), &configs[0..1]).await;
    let reply = replies.pop().unwrap();
    let effects = reply.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, _, _), _) = effects.created[0];

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let increment_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "Counter",
        "increment",
        package_ref,
        vec![CallArg::SharedObject(counter_id)],
    );

    // Let's submit the transaction to the first authority (the only one up-to-date).
    let reply =
        submit_shared_object_transaction(increment_counter_transaction.clone(), &configs[0..1])
            .await
            .pop()
            .unwrap();
    let info = reply.unwrap();
    let effects = info.signed_effects.unwrap().effects;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Let's submit the transaction to the out-of-date authorities.
    let replies =
        submit_shared_object_transaction(increment_counter_transaction.clone(), &configs[1..])
            .await;
    for reply in replies {
        match reply {
            Err(SuiError::SharedObjectLockingFailure(_)) => (),
            _ => panic!("Unexpected protocol message"),
        }
    }

    // Now send the missing certificates to the outdated authorities. We also re-send
    // the transaction to the first authority who should simply ignore it.
    tokio::task::yield_now().await;
    let replies = submit_single_owner_transaction(create_counter_transaction, &configs).await;
    for reply in replies {
        let effects = reply.signed_effects.unwrap().effects;
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    }

    // Now we can try again with the shared-object transaction who failed before.
    tokio::task::yield_now().await;
    let replies =
        submit_shared_object_transaction(increment_counter_transaction, &configs[1..]).await;
    for reply in replies {
        match reply {
            Ok(info) => {
                let effects = info.signed_effects.unwrap().effects;
                assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
            }
            Err(error) => panic!("{error}"),
        }
    }
}
