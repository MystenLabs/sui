// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

extern crate clap;
extern crate env_logger;
extern crate fastpay;
extern crate fastpay_core;

use fastpay::config::*;
use fastpay::{network, transport};
use fastpay_core::authority::*;
use fastpay_core::base_types::*;
use fastpay_core::client::*;
use fastpay_core::committee::Committee;
use fastpay_core::messages::*;
use fastpay_core::serialize::*;

use bytes::Bytes;
use clap::{App, Arg, SubCommand};
use futures::stream::StreamExt;
use log::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

fn make_authority_clients(
    committee_config: &CommitteeConfig,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
) -> HashMap<AuthorityName, network::Client> {
    let mut authority_clients = HashMap::new();
    for config in &committee_config.authorities {
        let config = config.clone();
        let client = network::Client::new(
            config.network_protocol,
            config.host,
            config.base_port,
            config.num_shards,
            buffer_size,
            send_timeout,
            recv_timeout,
        );
        authority_clients.insert(config.address, client);
    }
    authority_clients
}

fn make_authority_mass_clients(
    committee_config: &CommitteeConfig,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    max_in_flight: u64,
) -> Vec<(u32, network::MassClient)> {
    let mut authority_clients = Vec::new();
    for config in &committee_config.authorities {
        let client = network::MassClient::new(
            config.network_protocol,
            config.host.clone(),
            config.base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
            max_in_flight / config.num_shards as u64, // Distribute window to diff shards
        );
        authority_clients.push((config.num_shards, client));
    }
    authority_clients
}

fn make_client_state(
    accounts: &AccountsConfig,
    committee_config: &CommitteeConfig,
    address: FastPayAddress,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
) -> ClientState<network::Client> {
    let account = accounts.get(&address).expect("Unknown account");
    let committee = Committee::new(committee_config.voting_rights());
    let authority_clients =
        make_authority_clients(committee_config, buffer_size, send_timeout, recv_timeout);
    ClientState::new(
        address,
        account.key.copy(),
        committee,
        authority_clients,
        account.next_sequence_number,
        account.sent_certificates.clone(),
        account.received_certificates.clone(),
        account.balance,
    )
}

/// Make one transfer order per account, up to `max_orders` transfers.
fn make_benchmark_transfer_orders(
    accounts_config: &mut AccountsConfig,
    max_orders: usize,
) -> (Vec<TransferOrder>, Vec<(FastPayAddress, Bytes)>) {
    let mut orders = Vec::new();
    let mut serialized_orders = Vec::new();
    // TODO: deterministic sequence of orders to recover from interrupted benchmarks.
    let mut next_recipient = get_key_pair().0;
    for account in accounts_config.accounts_mut() {
        let transfer = Transfer {
            sender: account.address,
            recipient: Address::FastPay(next_recipient),
            amount: Amount::from(1),
            sequence_number: account.next_sequence_number,
            user_data: UserData::default(),
        };
        debug!("Preparing transfer order: {:?}", transfer);
        account.next_sequence_number = account.next_sequence_number.increment().unwrap();
        next_recipient = account.address;
        let order = TransferOrder::new(transfer.clone(), &account.key);
        orders.push(order.clone());
        let serialized_order = serialize_transfer_order(&order);
        serialized_orders.push((account.address, serialized_order.into()));
        if serialized_orders.len() >= max_orders {
            break;
        }
    }
    (orders, serialized_orders)
}

/// Try to make certificates from orders and server configs
fn make_benchmark_certificates_from_orders_and_server_configs(
    orders: Vec<TransferOrder>,
    server_config: Vec<&str>,
) -> Vec<(FastPayAddress, Bytes)> {
    let mut keys = Vec::new();
    for file in server_config {
        let server_config = AuthorityServerConfig::read(file).expect("Fail to read server config");
        keys.push((server_config.authority.address, server_config.key));
    }
    let mut serialized_certificates = Vec::new();
    for order in orders {
        let mut certificate = CertifiedTransferOrder {
            value: order.clone(),
            signatures: Vec::new(),
        };
        let committee = Committee {
            voting_rights: keys.iter().map(|(k, _)| (*k, 1)).collect(),
            total_votes: keys.len(),
        };
        for i in 0..committee.quorum_threshold() {
            let (pubx, secx) = keys.get(i).unwrap();
            let sig = Signature::new(&certificate.value, secx);
            certificate.signatures.push((*pubx, sig));
        }
        let serialized_certificate = serialize_cert(&certificate);
        serialized_certificates.push((order.transfer.sender, serialized_certificate.into()));
    }
    serialized_certificates
}

/// Try to aggregate votes into certificates.
fn make_benchmark_certificates_from_votes(
    committee_config: &CommitteeConfig,
    votes: Vec<SignedTransferOrder>,
) -> Vec<(FastPayAddress, Bytes)> {
    let committee = Committee::new(committee_config.voting_rights());
    let mut aggregators = HashMap::new();
    let mut certificates = Vec::new();
    let mut done_senders = HashSet::new();
    for vote in votes {
        // We aggregate votes indexed by sender.
        let address = vote.value.transfer.sender;
        if done_senders.contains(&address) {
            continue;
        }
        debug!(
            "Processing vote on {}'s transfer by {}",
            encode_address(&address),
            encode_address(&vote.authority)
        );
        let value = vote.value;
        let aggregator = aggregators
            .entry(address)
            .or_insert_with(|| SignatureAggregator::try_new(value, &committee).unwrap());
        match aggregator.append(vote.authority, vote.signature) {
            Ok(Some(certificate)) => {
                debug!("Found certificate: {:?}", certificate);
                let buf = serialize_cert(&certificate);
                certificates.push((address, buf.into()));
                done_senders.insert(address);
            }
            Ok(None) => {
                debug!("Added one vote");
            }
            Err(error) => {
                error!("Failed to aggregate vote: {}", error);
            }
        }
    }
    certificates
}

/// Broadcast a bulk of requests to each authority.
async fn mass_broadcast_orders(
    phase: &'static str,
    committee_config: &CommitteeConfig,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    max_in_flight: u64,
    orders: Vec<(FastPayAddress, Bytes)>,
) -> Vec<Bytes> {
    let time_start = Instant::now();
    info!("Broadcasting {} {} orders", orders.len(), phase);
    let authority_clients = make_authority_mass_clients(
        committee_config,
        buffer_size,
        send_timeout,
        recv_timeout,
        max_in_flight,
    );
    let mut streams = Vec::new();
    for (num_shards, client) in authority_clients {
        // Re-index orders by shard for this particular authority client.
        let mut sharded_requests = HashMap::new();
        for (address, buf) in &orders {
            let shard = AuthorityState::get_shard(num_shards, address);
            sharded_requests
                .entry(shard)
                .or_insert_with(Vec::new)
                .push(buf.clone());
        }
        streams.push(client.run(sharded_requests));
    }
    let responses = futures::stream::select_all(streams).concat().await;
    let time_elapsed = time_start.elapsed();
    warn!(
        "Received {} responses in {} ms.",
        responses.len(),
        time_elapsed.as_millis()
    );
    warn!(
        "Estimated server throughput: {} {} orders per sec",
        (orders.len() as u128) * 1_000_000 / time_elapsed.as_micros(),
        phase
    );
    responses
}

fn mass_update_recipients(
    accounts_config: &mut AccountsConfig,
    certificates: Vec<(FastPayAddress, Bytes)>,
) {
    for (_sender, buf) in certificates {
        if let Ok(SerializedMessage::Cert(certificate)) = deserialize_message(&buf[..]) {
            accounts_config.update_for_received_transfer(*certificate);
        }
    }
}

fn deserialize_response(response: &[u8]) -> Option<AccountInfoResponse> {
    match deserialize_message(response) {
        Ok(SerializedMessage::InfoResp(info)) => Some(*info),
        Ok(SerializedMessage::Error(error)) => {
            error!("Received error value: {}", error);
            None
        }
        Ok(_) => {
            error!("Unexpected return value");
            None
        }
        Err(error) => {
            error!(
                "Unexpected error: {} while deserializing {:?}",
                error, response
            );
            None
        }
    }
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let matches = App::new("FastPay client")
        .about("A Byzantine fault tolerant payments sidechain with low-latency finality and high throughput")
        .args_from_usage("
            --accounts=<FILE>     'Sets the file storing the state of our user accounts (an empty one will be created if missing)'
            --committee=<CONFIG>  'Sets the file describing the public configurations of all authorities'
        ")
        .arg(
            Arg::with_name("send_timeout")
                .long("send_timeout")
                .help("Timeout for sending queries (us)")
                .default_value("4000000"),
        )
        .arg(
            Arg::with_name("recv_timeout")
                .long("recv_timeout")
                .help("Timeout for receiving responses (us)")
                .default_value("4000000"),
        )
        .arg(
            Arg::with_name("buffer_size")
                .long("buffer_size")
                .help("Maximum size of datagrams received and sent (bytes")
                .default_value(transport::DEFAULT_MAX_DATAGRAM_SIZE),
        )
        .subcommand(SubCommand::with_name("transfer")
            .about("Transfer funds")
            .args_from_usage("
            --from=<ADDRESS>     'Sending address (must be one of our accounts)'
            --to=<ADDRESS>       'Recipient address'
            <amount>             'Amount to transfer'
            "))
        .subcommand(SubCommand::with_name("query_balance")
            .about("Obtain the spendable balance")
            .args_from_usage("
            <address>            'Address of the account'
            "))
        .subcommand(SubCommand::with_name("benchmark")
            .about("Send one transfer per account in bulk mode")
            .arg(
                Arg::with_name("max_in_flight")
                    .long("max_in_flight")
                    .help("Maximum number of requests in flight")
                    .default_value("200"),
            )
            .arg(
                Arg::with_name("max_orders")
                    .long("max_orders")
                    .help("Use a subset of the accounts to generate N transfers")
                    .default_value(""),
            )
            .arg(
                Arg::with_name("server_configs")
                    .long("server_configs")
                    .help("Use server configuration files to generate certificates (instead of aggregating received votes).")
                    .min_values(1),
            ))
        .subcommand(SubCommand::with_name("create_accounts")
            .about("Create new user accounts and print the public keys")
            .arg(
                Arg::with_name("initial_funding")
                    .long("initial_funding")
                    .help("known initial balance of the account")
                    .default_value("0"),
            )
            .args_from_usage("
            <num>                'Number of additional accounts to create'
            "))
        .get_matches();

    let send_timeout =
        Duration::from_micros(matches.value_of("send_timeout").unwrap().parse().unwrap());
    let recv_timeout =
        Duration::from_micros(matches.value_of("recv_timeout").unwrap().parse().unwrap());
    let accounts_config_path = matches.value_of("accounts").unwrap();
    let committee_config_path = matches.value_of("committee").unwrap();
    let buffer_size = matches
        .value_of("buffer_size")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    let mut accounts_config =
        AccountsConfig::read_or_create(accounts_config_path).expect("Unable to read user accounts");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Unable to read committee config file");

    match matches.subcommand() {
        ("transfer", Some(subm)) => {
            let sender = decode_address(&subm.value_of("from").unwrap().to_string())
                .expect("Failed to decode sender's address");
            let recipient = decode_address(&subm.value_of("to").unwrap().to_string())
                .expect("Failed to decode recipient's address");
            let amount = Amount::from(subm.value_of("amount").unwrap().parse::<u64>().unwrap());

            let mut rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let mut client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    sender,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );
                info!("Starting transfer");
                let time_start = Instant::now();
                let cert = client_state
                    .transfer_to_fastpay(amount, recipient, UserData::default())
                    .await
                    .unwrap();
                let time_total = time_start.elapsed().as_micros();
                info!("Transfer confirmed after {} us", time_total);
                println!("{:?}", cert);
                accounts_config.update_from_state(&client_state);
                info!("Updating recipient's local balance");
                let mut recipient_client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    recipient,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );
                recipient_client_state
                    .receive_from_fastpay(cert)
                    .await
                    .unwrap();
                accounts_config.update_from_state(&recipient_client_state);
                accounts_config
                    .write(accounts_config_path)
                    .expect("Unable to write user accounts");
                info!("Saved user account states");
            });
        }
        ("query_balance", Some(subm)) => {
            let address = decode_address(&subm.value_of("address").unwrap().to_string())
                .expect("Failed to decode address");

            let mut rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let mut client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    address,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );
                info!("Starting balance query");
                let time_start = Instant::now();
                let amount = client_state.get_spendable_amount().await.unwrap();
                let time_total = time_start.elapsed().as_micros();
                info!("Balance confirmed after {} us", time_total);
                println!("{:?}", amount);
                accounts_config.update_from_state(&client_state);
                accounts_config
                    .write(accounts_config_path)
                    .expect("Unable to write user accounts");
                info!("Saved client account state");
            });
        }
        ("benchmark", Some(subm)) => {
            let max_in_flight: u64 = subm.value_of("max_in_flight").unwrap().parse().unwrap();
            let max_orders: usize = subm
                .value_of("max_orders")
                .unwrap()
                .parse()
                .unwrap_or_else(|_| accounts_config.num_accounts());
            let server_configs = if subm.is_present("server_configs") {
                let files: Vec<_> = subm.values_of("server_configs").unwrap().collect();
                Some(files)
            } else {
                None
            };

            let mut rt = Runtime::new().unwrap();
            rt.block_on(async move {
                warn!("Starting benchmark phase 1 (transfer orders)");
                let (orders, serialize_orders) =
                    make_benchmark_transfer_orders(&mut accounts_config, max_orders);
                let responses = mass_broadcast_orders(
                    "transfer",
                    &committee_config,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                    max_in_flight,
                    serialize_orders,
                )
                .await;
                let votes: Vec<_> = responses
                    .into_iter()
                    .filter_map(|buf| {
                        deserialize_response(&buf[..]).and_then(|info| info.pending_confirmation)
                    })
                    .collect();
                warn!("Received {} valid votes.", votes.len());

                warn!("Starting benchmark phase 2 (confirmation orders)");
                let certificates = if let Some(files) = server_configs {
                    make_benchmark_certificates_from_orders_and_server_configs(orders, files)
                } else {
                    make_benchmark_certificates_from_votes(&committee_config, votes)
                };
                let responses = mass_broadcast_orders(
                    "confirmation",
                    &committee_config,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                    max_in_flight,
                    certificates.clone(),
                )
                .await;
                let mut confirmed = HashSet::new();
                let num_valid =
                    responses
                        .iter()
                        .fold(0, |acc, buf| match deserialize_response(&buf[..]) {
                            Some(info) => {
                                confirmed.insert(info.sender);
                                acc + 1
                            }
                            None => acc,
                        });
                warn!(
                    "Received {} valid confirmations for {} transfers.",
                    num_valid,
                    confirmed.len()
                );

                warn!("Updating local state of user accounts");
                // Make sure that the local balances are accurate so that future
                // balance checks of the non-mass client pass.
                mass_update_recipients(&mut accounts_config, certificates);
                accounts_config
                    .write(accounts_config_path)
                    .expect("Unable to write user accounts");
                info!("Saved client account state");
            });
        }
        ("create_accounts", Some(subm)) => {
            let num_accounts: u32 = subm.value_of("num").unwrap().parse().unwrap();
            let known_initial_funding = subm
                .value_of("initial_funding")
                .unwrap()
                .parse::<i128>()
                .unwrap();

            for _ in 0..num_accounts {
                let account = UserAccount::new(Balance::from(known_initial_funding));
                println!("{}", encode_address(&account.address));
                accounts_config.insert(account);
            }
            accounts_config
                .write(accounts_config_path)
                .expect("Unable to write user accounts");
        }
        _ => {
            error!("Unknown command");
        }
    }
}
