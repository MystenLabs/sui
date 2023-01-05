// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::authority_tests::init_state_with_object_id,
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
};
use sui_types::{
    base_types::{dbg_addr, dbg_object_id},
    object::ObjectFormatOptions,
};

//This is the most basic example of how to test the server logic
#[tokio::test]
async fn test_simple_request() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();

    let server = AuthorityServer::new_for_test(
        "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
        authority_state,
        consensus_address,
    );

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(server_handle.address())
        .await
        .unwrap();

    let req = ObjectInfoRequest::latest_object_info_request(
        object_id,
        Some(ObjectFormatOptions::default()),
    );

    client.handle_object_info_request(req).await.unwrap();
}
