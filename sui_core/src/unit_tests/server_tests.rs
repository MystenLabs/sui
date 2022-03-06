// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::base_types::{dbg_addr, dbg_object_id};
use sui_types::serialize::serialize_object_info_request;
use sui_types::object::ObjectFormatOptions;
use crate::authority::authority_tests::init_state_with_object_id;



use super::*;

#[tokio::test]
async fn test_start_stop_batch_subsystem() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    let mut server = AuthorityServer::new("127.0.0.1".to_string(), 999, 65000, authority_state);
    let join = server
        .spawn_batch_subsystem(1000, Duration::from_secs(5))
        .await
        .expect("Problem launching subsystem.");

    // Now drop the server to simulate the authority server ending processing.
    drop(server);

    // This should return immediately.
    join.await.expect("Error stoping subsystem");
}

// Some infra to feed the server messages and receive responses.

use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt};
use futures::sink::SinkMapErr;
use futures::channel::mpsc::{channel, Sender, Receiver};

struct TestChannel {
    reader : Receiver<Result<BytesMut, std::io::Error>>,
    writer: SinkMapErr<Sender<Bytes>, fn(<Sender<Bytes> as Sink<Bytes>>::Error)->std::io::Error>,
}

impl TestChannel {

    pub fn new() -> (TestChannel, (Sender<Result<BytesMut, std::io::Error>>, Receiver<Bytes>)) {
        let (outer_tx, inner_rx) = channel(1000);
        let (inner_tx, outer_rx) = channel(1000);

        let test_channel = TestChannel {
            reader: inner_rx,
            writer: inner_tx.sink_map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "SOme error!")),
        };

        (test_channel, (outer_tx, outer_rx))
    }
}

impl<'a> RwChannel<'a> for TestChannel {

    type R = Receiver<Result<BytesMut, std::io::Error>>;
    type W = SinkMapErr<Sender<Bytes>, fn(<Sender<Bytes> as Sink<Bytes>>::Error)->std::io::Error>;


    fn sink(&mut self) -> &mut Self::W {
        &mut self.writer
    }
    fn stream(&mut self) -> &mut Self::R{
        &mut self.reader
    }
}

//This is the most basic example of how to test the server logic

#[tokio::test]
async fn test_channel_infra() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    let mut server = Arc::new(AuthorityServer::new("127.0.0.1".to_string(), 999, 65000, authority_state));

    let (channel, (mut tx, mut rx)) = TestChannel::new();

    let handle = tokio::spawn(async move {
        server.handle_messages(channel).await;
    });

    let req = ObjectInfoRequest::latest_object_info_request(
        object_id,
        Some(ObjectFormatOptions::default()));

    let bytes : BytesMut = BytesMut::from(&serialize_object_info_request(&req)[..]);
    tx.send(Ok(bytes)).await.expect("Problem sending");
    let resp = rx.next().await;
    assert!(resp.unwrap().len() > 0);

    drop(tx);
    handle.await.expect("Problem closing task");
}