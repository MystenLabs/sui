// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{dbg_addr, dbg_object_id};

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
        .expect("No issues launching subsystem.");

    // Now drop the server to simulate the authority server ending processing.
    drop(server);

    // This should return immediately.
    join.await.expect("Error stoping subsystem");
}
