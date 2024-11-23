// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use sui_graphql_rpc::commands::Command;
use sui_graphql_rpc::config::{ServerConfig, ServiceConfig, Version};
use sui_graphql_rpc::server::graphiql_server::start_graphiql_server;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

// Define the `GIT_REVISION` const
bin_version::git_revision!();

// VERSION mimics what other sui binaries use for the same const
static VERSION: Version = Version {
    major: env!("CARGO_PKG_VERSION_MAJOR"),
    minor: env!("CARGO_PKG_VERSION_MINOR"),
    patch: env!("CARGO_PKG_VERSION_PATCH"),
    sha: GIT_REVISION,
    full: const_str::concat!(
        env!("CARGO_PKG_VERSION_MAJOR"),
        ".",
        env!("CARGO_PKG_VERSION_MINOR"),
        ".",
        env!("CARGO_PKG_VERSION_PATCH"),
        "-",
        GIT_REVISION
    ),
};

#[tokio::main]
async fn main() {
    let cmd: Command = Command::parse();
    match cmd {
        Command::GenerateConfig { output } => {
            let config = ServiceConfig::default();
            let toml = toml::to_string_pretty(&config).expect("Failed to serialize configuration");

            if let Some(path) = output {
                fs::write(&path, toml).unwrap_or_else(|e| {
                    panic!("Failed to write configuration to {}: {e}", path.display())
                });
            } else {
                println!("{}", toml);
            }
        }

        Command::StartServer {
            ide,
            connection,
            config,
            tx_exec_full_node,
        } => {
            let service_config = service_config(config);
            let _guard = telemetry_subscribers::TelemetryConfig::new()
                .with_env()
                .init();
            let tracker = TaskTracker::new();
            let cancellation_token = CancellationToken::new();

            println!("Starting server...");
            let server_config = ServerConfig {
                connection,
                service: service_config,
                ide,
                tx_exec_full_node,
                ..ServerConfig::default()
            };

            let cancellation_token_clone = cancellation_token.clone();
            let graphql_service_handle = tracker.spawn(async move {
                start_graphiql_server(&server_config, &VERSION, cancellation_token_clone)
                    .await
                    .unwrap();
            });

            // Wait for shutdown signal
            tokio::select! {
                result = graphql_service_handle => {
                    if let Err(e) = result {
                        println!("GraphQL service crashed or exited with error: {:?}", e);
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("Ctrl+C signal received.");
                },
            }

            println!("Shutting down...");

            // Send shutdown signal to application
            cancellation_token.cancel();
            tracker.close();
            tracker.wait().await;
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
