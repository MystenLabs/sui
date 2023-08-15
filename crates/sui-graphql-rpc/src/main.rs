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
        } => {
            let config = ServerConfig {
                port,
                host,
                rpc_url,
            };

            println!("Starting server...");
            start_example_server(Some(config)).await;
        }
    }
}
