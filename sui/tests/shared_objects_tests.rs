// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::{sink::SinkExt, stream::StreamExt};
use std::net::SocketAddr;
use std::time::Duration;
use sui_types::messages::ConsensusTransaction;
use sui_types::serialize::{deserialize_message, serialize_consensus_transaction};
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::test_shared_object_certificates;
use test_utils::objects::{test_gas_objects, test_shared_object};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;
use tokio_util::codec::LengthDelimitedCodec;

#[tokio::test]
async fn shared_object_transaction() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    let mut configs = test_authority_configs();
    spawn_test_authorities(objects, &configs).await;

    tokio::task::yield_now().await;
    let config = configs.pop().unwrap();

    let authority_address: SocketAddr = format!("{}:{}", config.host, config.port).parse().unwrap();
    let stream = TcpStream::connect(authority_address).await.unwrap();
    let mut connection = Framed::new(stream, LengthDelimitedCodec::new());

    let certificate = test_shared_object_certificates().await.pop().unwrap();
    let message = ConsensusTransaction::UserTransaction(certificate);
    let serialized = serialize_consensus_transaction(&message);

    tokio::time::sleep(Duration::from_millis(1_000)).await;

    println!("UNIT_TEST: 0");
    connection.send(Bytes::from(serialized)).await.unwrap();
    println!("UNIT_TEST: 1");
    let bytes = connection.next().await.unwrap().unwrap();
    println!("UNIT_TEST: 2");
    let reply = deserialize_message(&bytes[..]).unwrap();
    println!("{reply:?}");
}
