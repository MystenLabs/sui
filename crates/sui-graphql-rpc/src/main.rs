// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::{env, fs};

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::config::{ConnectionConfig, DataSourceConfig, ServiceConfig};
use sui_graphql_rpc::schema_sdl_export;
use sui_graphql_rpc::server::simple_server::start_example_server;

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
            db_url,
            pool_size,
            connection_timeout,
            statement_timeout,
            config,
        } => {
            let db_url = db_url.or_else(|| env::var("PG_DB_URL").ok());

            let datasource_config = DataSourceConfig::new(
                rpc_url,
                db_url,
                pool_size,
                connection_timeout,
                statement_timeout,
            );
            let conn = ConnectionConfig::new(port, host);

            let service_config = service_config(config);

            println!("Starting server...");
            let result = start_example_server(conn, datasource_config, service_config).await;
            println!("Server stopped: {:?}", result);
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
