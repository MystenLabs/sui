// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

mod cli_pretty;
mod client_api;
mod client_api_helpers;
use fastpay::{config::*, transport};
use fastx_types::base_types::*;

use log::*;
use std::time::Duration;
use structopt::StructOpt;

fn parse_public_key_bytes(src: &str) -> Result<PublicKeyBytes, hex::FromHexError> {
    decode_address_hex(src)
}

#[derive(StructOpt)]
#[structopt(
    name = "FastPay Client",
    about = "A Byzantine fault tolerant payments sidechain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct ClientOpt {
    /// Sets the file storing the state of our user accounts (an empty one will be created if missing)
    #[structopt(long)]
    accounts: String,

    /// Sets the file describing the public configurations of all authorities
    #[structopt(long)]
    committee: String,

    /// Timeout for sending queries (us)
    #[structopt(long, default_value = "4000000")]
    send_timeout: u64,

    /// Timeout for receiving responses (us)
    #[structopt(long, default_value = "4000000")]
    recv_timeout: u64,

    /// Maximum size of datagrams received and sent (bytes)
    #[structopt(long, default_value = transport::DEFAULT_MAX_DATAGRAM_SIZE)]
    buffer_size: usize,

    /// Subcommands. Acceptable values are transfer, query_objects, benchmark, and create_accounts.
    #[structopt(subcommand)]
    cmd: ClientCommands,

    /// Pretty print output
    #[structopt(long)]
    pretty: bool,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum ClientCommands {
    /// Get obj info
    #[structopt(name = "get-obj-info")]
    GetObjInfo { obj_id: ObjectID },

    /// Call Move
    #[structopt(name = "call")]
    Call { path: String },

    /// Transfer funds
    #[structopt(name = "transfer")]
    Transfer {
        /// Sending address (must be one of our accounts)
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        from: PublicKeyBytes,

        /// Recipient address
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        to: PublicKeyBytes,

        /// Object to transfer, in 20 bytes Hex string
        object_id: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        gas_object_id: ObjectID,
    },

    /// Obtain the Account Addresses
    #[structopt(name = "query-accounts-addrs")]
    QueryAccountAddresses {},

    /// Obtain the Object Info
    #[structopt(name = "query-objects")]
    QueryObjects {
        /// Address of the account
        #[structopt(long, parse(try_from_str = parse_public_key_bytes))]
        address: PublicKeyBytes,
    },

    /// Send one transfer per account in bulk mode
    #[structopt(name = "benchmark")]
    Benchmark {
        /// Maximum number of requests in flight
        #[structopt(long, default_value = "200")]
        max_in_flight: u64,

        /// Use a subset of the accounts to generate N transfers
        #[structopt(long)]
        max_orders: Option<usize>,

        /// Use server configuration files to generate certificates (instead of aggregating received votes).
        #[structopt(long)]
        server_configs: Option<Vec<String>>,
    },

    /// Create new user accounts with randomly generated object IDs
    #[structopt(name = "create-accounts")]
    CreateAccounts {
        /// Number of additional accounts to create
        #[structopt(long, default_value = "1000")]
        num: u32,

        /// Number of objects per account
        #[structopt(long, default_value = "1000")]
        gas_objs_per_account: u32,

        /// Gas value per object
        #[structopt(long, default_value = "1000")]
        #[allow(dead_code)]
        value_per_per_obj: u32,

        /// Initial state config file path
        #[structopt(name = "init-state-cfg")]
        initial_state_config_path: String,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = ClientOpt::from_args();
    let send_timeout = Duration::from_micros(options.send_timeout);
    let recv_timeout = Duration::from_micros(options.recv_timeout);
    let accounts_config_path = &options.accounts;
    let committee_config_path = &options.committee;
    let buffer_size = options.buffer_size;
    let pretty = options.pretty;

    let mut accounts_config =
        AccountsConfig::read_or_create(accounts_config_path).expect("Unable to read user accounts");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Unable to read committee config file");

    match options.cmd {
        ClientCommands::GetObjInfo { obj_id } => {
            let obj_info = client_api::get_object_info(
                obj_id,
                &mut accounts_config,
                &committee_config,
                send_timeout,
                recv_timeout,
                buffer_size,
            );
            cli_pretty::format_obj_info_response(&obj_info);
        }

        ClientCommands::Call { path } => {
            let config = MoveCallConfig::read(&path).unwrap();

            let (_, order_effetcs) = client_api::move_call(
                config,
                &mut accounts_config,
                &committee_config,
                send_timeout,
                recv_timeout,
                buffer_size,
            );
            cli_pretty::format_order_effects(&order_effetcs);
        }

        ClientCommands::Transfer {
            from,
            to,
            object_id,
            gas_object_id,
        } => {
            let cert = client_api::transfer_object(
                to,
                from,
                object_id,
                gas_object_id,
                &mut accounts_config,
                &committee_config,
                send_timeout,
                recv_timeout,
                buffer_size,
            );
            info!("Transfer confirmed");
            println!("{:?}", cert);
            info!("Updating recipient's local balance");
        }

        ClientCommands::QueryAccountAddresses {} => {
            let addr_strings: Vec<_> = accounts_config
                .addresses()
                .into_iter()
                .map(|addr| format!("{:X}", addr).trim_end_matches('=').to_string())
                .collect();
            let addr_text = addr_strings.join("\n");
            println!("{}", addr_text);
        }

        ClientCommands::QueryObjects { address } => {
            let obj_map = client_api::get_account_objects(
                address,
                send_timeout,
                recv_timeout,
                buffer_size,
                &mut accounts_config,
                &committee_config,
            );

            if pretty {
                cli_pretty::format_objects(&obj_map).printstd();
            } else {
                for (obj_id, seq_num) in &obj_map {
                    println!("{}: {:?}", obj_id, seq_num);
                }
            }
        }

        ClientCommands::Benchmark {
            max_in_flight,
            max_orders,
            server_configs,
        } => {
            client_api::benchmark(
                &mut accounts_config,
                &committee_config,
                send_timeout,
                recv_timeout,
                buffer_size,
                max_in_flight,
                max_orders,
                server_configs,
            );
        }

        ClientCommands::CreateAccounts {
            num,
            gas_objs_per_account,
            // TODO: Integrate gas logic with https://github.com/MystenLabs/fastnft/pull/97
            value_per_per_obj,
            initial_state_config_path,
        } => {
            let acc_cfgs = client_api::create_account_configs(
                &mut accounts_config,
                num.try_into().unwrap(),
                value_per_per_obj,
                gas_objs_per_account,
            );
            acc_cfgs
                .write(initial_state_config_path.as_str())
                .expect("Unable to write to initial state config file");

            cli_pretty::format_account_configs_create(acc_cfgs);
        }
    }

    accounts_config
        .write(accounts_config_path)
        .expect("Unable to write user accounts");
}
