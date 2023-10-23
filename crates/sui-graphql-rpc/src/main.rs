// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::config::{ConnectionConfig, ServiceConfig};
use sui_graphql_rpc::schema_sdl_export;
use sui_graphql_rpc::server::builder::Server;
use sui_graphql_rpc::server::simple_server::start_example_server;
use tracing::error;

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
            db_url,
            port,
            host,
            config,
            prom_host,
            prom_port,
        } => {
            let conn = ConnectionConfig::new(port, host, rpc_url, db_url, prom_host, prom_port);
            let service_config = service_config(config);

            println!("Starting server...");
            start_example_server(conn, service_config).await.unwrap();
        }
        Command::FromConfig { path } => {
            let server = Server::from_yaml_config(path.to_str().unwrap());
            println!("Starting server...");
            server
                .await
                .map_err(|x| {
                    error!("Error: {:?}", x);
                    x
                })
                .unwrap()
                .run()
                .await
                .unwrap();
        }
    }
}

fn service_config(path: Option<PathBuf>) -> ServiceConfig {
    let Some(path) = path else {
        return ServiceConfig::default();
    };

    let contents = fs::read_to_string(path).expect("Reading configuration");
    ServiceConfig::read(&contents).expect("Deserializing configuration")
}
