// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::ObjectID,
    digests::TransactionDigest,
    object::{bounded_visitor::BoundedVisitor, Data},
};

use sui_package_resolver::Resolver;

use clap::{Parser, Subcommand};
use std::{fs, path::PathBuf, str::FromStr};
use sui_light_client::checkpoint::check_and_sync_checkpoints;
use sui_light_client::config::Config;
use sui_light_client::package_store::RemotePackageStore;
use sui_light_client::verifier::{get_verified_effects_and_events, get_verified_object};

use tracing::info;

/// A light client for the Sui blockchain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<SCommands>,
}

#[derive(Subcommand, Debug)]
enum SCommands {
    /// Sync all end-of-epoch checkpoints
    Sync {},

    /// Checks a specific transaction using the light client
    Transaction {
        /// Transaction hash
        #[arg(short, long, value_name = "TID")]
        tid: String,
    },

    /// Checks a specific object using the light client
    Object {
        /// Transaction hash
        #[arg(short, long, value_name = "OID")]
        oid: String,
    },
}

#[tokio::main]
pub async fn main() {
    env_logger::init();

    // Command line arguments and config loading
    let args = Args::parse();

    let path = args
        .config
        .unwrap_or_else(|| panic!("Need a config file path"));
    let reader = fs::File::open(path.clone())
        .unwrap_or_else(|_| panic!("Unable to load config from {}", path.display()));
    let config: Config = serde_yaml::from_reader(reader).unwrap();

    // Print config parameters
    println!(
        "Checkpoint Dir: {}",
        config.checkpoint_summary_dir.display()
    );

    let remote_package_store = RemotePackageStore::new(config.clone());
    let resolver = Resolver::new(remote_package_store);

    match args.command {
        Some(SCommands::Transaction { tid }) => {
            let (effects, events) = get_verified_effects_and_events(
                &config,
                TransactionDigest::from_str(&tid).unwrap(),
            )
            .await
            .unwrap();

            let exec_digests = effects.execution_digests();
            println!(
                "Executed TID: {} Effects: {}",
                exec_digests.transaction, exec_digests.effects
            );

            if events.is_some() {
                for event in events.as_ref().unwrap().data.iter() {
                    let type_layout = resolver
                        .type_layout(event.type_.clone().into())
                        .await
                        .unwrap();

                    let result = BoundedVisitor::deserialize_value(&event.contents, &type_layout)
                        .expect("Cannot deserialize");

                    println!(
                        "Event:\n - Package: {}\n - Module: {}\n - Sender: {}\n - Type: {}\n{}",
                        event.package_id,
                        event.transaction_module,
                        event.sender,
                        event.type_,
                        serde_json::to_string_pretty(&result).unwrap()
                    );
                }
            } else {
                println!("No events found");
            }
        }
        Some(SCommands::Object { oid }) => {
            let oid = ObjectID::from_str(&oid).unwrap();
            let object = get_verified_object(&config, oid).await.unwrap();
            info!("Successfully verified object: {}", oid);

            if let Data::Move(move_object) = &object.data {
                let object_type = move_object.type_().clone();

                let type_layout = resolver
                    .type_layout(object_type.clone().into())
                    .await
                    .unwrap();

                let result =
                    BoundedVisitor::deserialize_value(move_object.contents(), &type_layout)
                        .expect("Cannot deserialize");

                let (oid, version, hash) = object.compute_object_reference();
                println!(
                    "OID: {}\n - Version: {}\n - Hash: {}\n - Owner: {}\n - Type: {}\n{}",
                    oid,
                    version,
                    hash,
                    object.owner,
                    object_type,
                    serde_json::to_string_pretty(&result).unwrap()
                );
            }
        }

        Some(SCommands::Sync {}) => {
            check_and_sync_checkpoints(&config)
                .await
                .expect("Failed to sync checkpoints");
        }
        _ => {
            println!("No command...");
        }
    }
}
