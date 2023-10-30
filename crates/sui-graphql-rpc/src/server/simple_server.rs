// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::ServerConfig;
use crate::server::builder::Server;

pub async fn start_example_server(server_config: &ServerConfig) -> Result<(), crate::error::Error> {
    println!("Starting server with config: {:?}", server_config);

    let server = Server::from_config(server_config).await?;

    println!(
        "Launch GraphiQL IDE at: http://{}",
        server_config.connection.server_address()
    );

    server.run().await
}
