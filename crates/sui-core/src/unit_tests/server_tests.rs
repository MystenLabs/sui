// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::authority_tests::init_state_with_object_id,
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
};
use sui_types::{
    base_types::{dbg_addr, dbg_object_id},
    messages_grpc::LayoutGenerationOption,
};

//This is the most basic example of how to test the server logic
#[tokio::test]
async fn test_simple_request() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);
    let authority_state = init_state_with_object_id(sender, object_id).await;

    // The following two fields are only needed for shared objects (not by this bench).
    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();

    let req =
        ObjectInfoRequest::latest_object_info_request(object_id, LayoutGenerationOption::Generate);

    client.handle_object_info_request(req).await.unwrap();
}
