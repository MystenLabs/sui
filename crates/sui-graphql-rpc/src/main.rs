// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::schema_sdl_export;
use sui_graphql_rpc::server::simple_server::start_example_server;
use sui_graphql_rpc::server::simple_server::ServerConfig;

#[tokio::main]
async fn main() {
    let cmd: Command = Command::parse();
    match cmd {
        Command::GenerateSchema { file } => {
            let out = schema_sdl_export();
            if let Some(file) = file {
                println!("Write schema to file: {:?}", file);
                std::fs::write(file, &out).unwrap();
            } else {
                println!("{}", &out);
            }
        }
        Command::StartServer {
            rpc_url,
            port,
            host,
            max_query_depth,
        } => {
            let mut config = ServerConfig::default();
            if let Some(rpc_url) = rpc_url {
                config.rpc_url = rpc_url;
            }
            if let Some(port) = port {
                config.port = port;
            }
            if let Some(host) = host {
                config.host = host;
            }
            if let Some(max_query_depth) = max_query_depth {
                config.max_query_depth = max_query_depth;
            }

            println!("Starting server...");
            start_example_server(Some(config)).await;
        }
    }
}
