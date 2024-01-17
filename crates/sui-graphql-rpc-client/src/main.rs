// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_graphql_rpc_client::simple_client::SimpleClient;

#[tokio::main]
async fn main() -> () {
    let server_url = "http://127.0.0.1:8000".to_string();

    // Starts graphql client
    let client = SimpleClient::new(server_url);

    client
        .get_coins(
            "0xdb7ad6cb4c71f8815f82066d47f76cfe968d997466e4699690ceaf50f025496f".to_string(),
            None,
            None,
        )
        .await;
}
