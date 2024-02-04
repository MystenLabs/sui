// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::config::{ConnectionConfig, ServerConfig, ServiceConfig};
use sui_graphql_rpc::config::{Ide, TxExecFullNodeConfig};
use sui_graphql_rpc::server::builder::export_schema;
use sui_graphql_rpc::server::graphiql_server::{
    start_graphiql_server, start_graphiql_server_from_cfg_path,
};
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
        Command::GenerateDocsExamples => {
            let mut buf: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            // we are looking to put examples content in
            // sui/docs/content/references/sui-graphql/examples.mdx
            let filename = "docs/content/references/sui-graphql/examples.mdx";
            buf.pop();
            buf.pop();
            buf.push(filename);
            let content = sui_graphql_rpc::examples::generate_examples_for_docs()
                .expect("Generating examples markdown file for docs failed");
            std::fs::write(buf, content).expect("Writing examples markdown failed");
            println!("Generated the docs example.mdx file and copied it to {filename}.");
        }
        Command::GenerateSchema { file } => {
            let out = export_schema();
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
            node_rpc_url,
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
                tx_exec_full_node: TxExecFullNodeConfig::new(node_rpc_url),
                ..ServerConfig::default()
            };

            start_graphiql_server(&server_config).await.unwrap();
        }
        Command::FromConfig { path } => {
            println!("Starting server...");
            start_graphiql_server_from_cfg_path(path.to_str().unwrap())
                .await
                .map_err(|x| {
                    error!("Error: {:?}", x);
                    x
                })
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
