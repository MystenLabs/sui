// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use sui::config::AuthorityPrivateInfo;
use sui_types::error::SuiError;
use sui_types::messages::ConsensusTransaction;
use sui_types::serialize::SerializedMessage;
use sui_types::serialize::{deserialize_message, serialize_consensus_transaction};
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::test_shared_object_certificates;
use test_utils::objects::{test_gas_objects, test_shared_object};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::codec::LengthDelimitedCodec;

/// Submits a transaction to a Sui authority.
async fn submit_transaction(
    transaction: Bytes,
    config: &AuthorityPrivateInfo,
) -> SerializedMessage {
    let authority_address = format!("{}:{}", config.host, config.port);
    let stream = TcpStream::connect(authority_address).await.unwrap();
    let mut connection = Framed::new(stream, LengthDelimitedCodec::new());

    connection.send(transaction).await.unwrap();
    let bytes = connection.next().await.unwrap().unwrap();
    deserialize_message(&bytes[..]).unwrap()
}

// TODO: Taking too long to run. Re-enable once it's fixed.
#[ignore]
#[tokio::test]
async fn shared_object_transaction() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let certificate = test_shared_object_certificates().await.pop().unwrap();
    let message = ConsensusTransaction::UserTransaction(certificate);
    let serialized = Bytes::from(serialize_consensus_transaction(&message));

    // Keep submitting the certificate until it is sequenced by consensus. We use the loop
    // since some consensus protocols (like Tusk) are not guaranteed to include the transaction
    // (but it has high probability to do so).
    tokio::task::yield_now().await;
    'main: loop {
        for config in &configs {
            match submit_transaction(serialized.clone(), config).await {
                SerializedMessage::TransactionResp(_) => {
                    // We got a reply from the Sui authority.
                    break 'main;
                }
                SerializedMessage::Error(error) => match *error {
                    SuiError::ConsensusConnectionBroken(_) => {
                        // This is the (confusing) error message returned by the consensus adapter
                        // timed out and didn't hear back from consensus.
                    }
                    error => panic!("Unexpected error {error}"),
                },
                message => panic!("Unexpected protocol message {message:?}"),
            }
        }
    }
}
