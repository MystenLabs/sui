// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::{config::*, transport};
use fastx_types::base_types::*;
mod server_api;

use log::*;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fs};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "FastPay Server",
    about = "A byzantine fault tolerant payments sidechain with low-latency finality and high throughput"
)]
struct ServerOpt {
    /// Path to the file containing the server configuration of this FastPay authority (including its secret key)
    #[structopt(long)]
    server: String,

    /// Subcommands. Acceptable values are run and generate.
    #[structopt(subcommand)]
    cmd: ServerCommands,
}

#[derive(StructOpt)]
enum ServerCommands {
    /// Runs the FastPay authority")
    #[structopt(name = "run")]
    Run {
        /// Maximum size of datagrams received and sent (bytes)
        #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE)]
        buffer_size: usize,

        /// Path to the file containing the public description of all authorities in this FastPay committee
        #[structopt(long)]
        committee: String,

        /// Path to the file describing the initial user accounts
        #[structopt(long)]
        initial_accounts: String,

        /// Sets the local ip addr
        #[structopt(long)]
        local_ip: String,
    },

    /// Generate a new server configuration and output its public description
    #[structopt(name = "generate")]
    Generate {
        /// Chooses a network protocol between Udp and Tcp
        #[structopt(long, default_value = "Udp")]
        protocol: transport::NetworkProtocol,

        /// Sets the public name of the host
        #[structopt(long)]
        host: String,

        /// Sets the port
        #[structopt(long)]
        port: u32,

        /// Sets the path to the database folder
        #[structopt(long, default_value = "")]
        database_path: String,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = ServerOpt::from_args();

    let server_config_path = &options.server;

    match options.cmd {
        ServerCommands::Run {
            buffer_size,
            committee,
            initial_accounts,
            local_ip,
        } => {
            let (auth_cfg, committee_cfg, init_config) =
                read_cfg_files(server_config_path, &committee, &initial_accounts);

            server_api::run_server(&local_ip, auth_cfg, committee_cfg, init_config, buffer_size);
        }

        ServerCommands::Generate {
            protocol,
            host,
            port,
            database_path,
        } => {
            // Set the path to the DB
            let db_path_str = if database_path.is_empty() {
                env::temp_dir().join(format!("DB_{:?}", ObjectID::random()))
            } else {
                PathBuf::from_str(&database_path).unwrap()
            };
            let db_path = Path::new(&db_path_str);
            fs::create_dir(&db_path).unwrap();
            info!("Will open database on path: {:?}", db_path.as_os_str());

            // The configuration of this authority
            let authority_config = server_api::create_server_configs(
                protocol,
                host,
                port,
                db_path.to_str().unwrap().to_string(),
            );

            // Write to the store
            authority_config
                .write(server_config_path)
                .expect("Unable to write server config file");
            info!("Wrote server config file");
            authority_config.authority.print();
        }
    }
}

fn read_cfg_files(
    server_config_path: &str,
    committee_config_path: &str,
    initial_accounts_config_path: &str,
) -> (AuthorityServerConfig, CommitteeConfig, InitialStateConfig) {
    let server_config =
        AuthorityServerConfig::read(server_config_path).expect("Fail to read server config");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Fail to read committee config");
    let initial_accounts_config = InitialStateConfig::read(initial_accounts_config_path)
        .expect("Fail to read initial account config");

    (server_config, committee_config, initial_accounts_config)
}
