// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::config::Ide;
use sui_graphql_rpc::config::{ConnectionConfig, ServerConfig, ServiceConfig};
use sui_graphql_rpc::schema_sdl_export;
use sui_graphql_rpc::server::builder::Server;
use sui_graphql_rpc::server::simple_server::start_example_server;
use tracing::error;

#[tokio::main]
async fn main() {
    let cmd: Command = Command::parse();
    match cmd {
        Command::GenerateConfig { path } => {
            let cfg = ServerConfig::default();
            if let Some(file) = path {
                println!("Write config to file: {:?}", file);
                cfg.to_yaml_file(file)
                    .expect("Failed writing config to file");
            } else {
                println!(
                    "{}",
                    &cfg.to_yaml().expect("Failed serializing config to yaml")
                );
            }
        }
        Command::GenerateSchema { file } => {
            let out = schema_sdl_export();
            if let Some(file) = file {
                println!("Write schema to file: {:?}", file);
                std::fs::write(file, &out).unwrap();
            } else {
                println!("{}", &out);
            }
        }
        Command::GenerateExamples { file } => {
            let new_content: String = sui_graphql_rpc::examples::generate_markdown()
                .expect("Generating examples markdown failed");

            let mut buf: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            buf.push("docs");
            buf.push("examples.md");
            let file = file.unwrap_or(buf);

            std::fs::write(file.clone(), new_content).expect("Writing examples markdown failed");
            println!("Written examples to file: {:?}", file);
        }
        Command::StartServer {
            ide_title,
            db_url,
            port,
            host,
            config,
            prom_host,
            prom_port,
        } => {
            let connection = ConnectionConfig::new(port, host, db_url, None, prom_host, prom_port);
            let service_config = service_config(config);
            let _guard = telemetry_subscribers::TelemetryConfig::new()
                .with_env()
                .init();

            println!("Starting server...");
            let server_config = ServerConfig {
                connection,
                service: service_config,
                ide: Ide::new(ide_title),
                ..ServerConfig::default()
            };

            start_example_server(&server_config).await.unwrap();
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
