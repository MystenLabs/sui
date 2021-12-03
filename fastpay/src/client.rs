// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::{config::*, network, transport};
use fastpay_core::{
    authority::*, base_types::*, client::*, committee::Committee, messages::*, serialize::*,
};

use bytes::Bytes;
use futures::stream::StreamExt;
use log::*;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use structopt::StructOpt;
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
        account.object_ids.clone(),
    )
}

/// Make one transfer order per account, up to `max_orders` transfers.
fn make_benchmark_transfer_orders(
    accounts_config: &mut AccountsConfig,
    max_orders: usize,
) -> (Vec<Order>, Vec<(FastPayAddress, ObjectID, Bytes)>) {
    let mut orders = Vec::new();
    let mut serialized_orders = Vec::new();
    // TODO: deterministic sequence of orders to recover from interrupted benchmarks.
    let mut next_recipient = get_key_pair().0;
    for account in accounts_config.accounts_mut() {
        let object_id = *account.object_ids.first().unwrap();
        let transfer = Transfer {
            object_id,
            sender: account.address,
            recipient: Address::FastPay(next_recipient),
            sequence_number: account.next_sequence_number,
            user_data: UserData::default(),
        };
        debug!("Preparing transfer order: {:?}", transfer);
        account.next_sequence_number = account.next_sequence_number.increment().unwrap();
        next_recipient = account.address;
        let order = Order::new_transfer(transfer.clone(), &account.key);
        orders.push(order.clone());
        let serialized_order = serialize_order(&order);
        serialized_orders.push((account.address, object_id, serialized_order.into()));
        if serialized_orders.len() >= max_orders {
            break;
        }
    }
    (orders, serialized_orders)
}

/// Try to make certificates from orders and server configs
fn make_benchmark_certificates_from_orders_and_server_configs(
    orders: Vec<Order>,
    server_config: Vec<&str>,
) -> Vec<(FastPayAddress, ObjectID, Bytes)> {
    let mut keys = Vec::new();
    for file in server_config {
        let server_config = AuthorityServerConfig::read(file).expect("Fail to read server config");
        keys.push((server_config.authority.address, server_config.key));
    }
    let committee = Committee {
        voting_rights: keys.iter().map(|(k, _)| (*k, 1)).collect(),
        total_votes: keys.len(),
    };
    assert!(
        keys.len() >= committee.quorum_threshold(),
        "Not enough server configs were provided with --server-configs"
    );
    let mut serialized_certificates = Vec::new();
    for order in orders {
        let mut certificate = CertifiedOrder {
            order: order.clone(),
            signatures: Vec::new(),
        };
        for i in 0..committee.quorum_threshold() {
            let (pubx, secx) = keys.get(i).unwrap();
            let sig = Signature::new(&certificate.order.kind, secx);
            certificate.signatures.push((*pubx, sig));
        }
        let serialized_certificate = serialize_cert(&certificate);
        serialized_certificates.push((
            *order.sender(),
            *order.object_id(),
            serialized_certificate.into(),
        ));
    }
    serialized_certificates
}

/// Try to aggregate votes into certificates.
fn make_benchmark_certificates_from_votes(
    committee_config: &CommitteeConfig,
    votes: Vec<SignedOrder>,
) -> Vec<(FastPayAddress, ObjectID, Bytes)> {
    let committee = Committee::new(committee_config.voting_rights());
    let mut aggregators = HashMap::new();
    let mut certificates = Vec::new();
    let mut done_senders = HashSet::new();
    for vote in votes {
        // We aggregate votes indexed by sender.
        let address = *vote.order.sender();
        let object_id = *vote.order.object_id();
        if done_senders.contains(&address) {
            continue;
        }
        debug!(
            "Processing vote on {}'s transfer by {}",
            encode_address(&address),
            encode_address(&vote.authority)
        );
        let value = vote.order;
        let aggregator = aggregators
            .entry(address)
            .or_insert_with(|| SignatureAggregator::try_new(value, &committee).unwrap());
        match aggregator.append(vote.authority, vote.signature) {
            Ok(Some(certificate)) => {
                debug!("Found certificate: {:?}", certificate);
                let buf = serialize_cert(&certificate);
                certificates.push((address, object_id, buf.into()));
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
    orders: Vec<(FastPayAddress, ObjectID, Bytes)>,
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
        for (_, object_id, buf) in &orders {
            let shard = AuthorityState::get_shard(num_shards, object_id);
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
    certificates: Vec<(FastPayAddress, ObjectID, Bytes)>,
) {
    for (_sender, _object_id, buf) in certificates {
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

#[derive(StructOpt)]
#[structopt(
    name = "FastPay Client",
    about = "A Byzantine fault tolerant payments sidechain with low-latency finality and high throughput"
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

    /// Subcommands. Acceptable values are transfer, query_balance, benchmark, and create_accounts.
    #[structopt(subcommand)]
    cmd: ClientCommands,
}

#[derive(StructOpt)]
enum ClientCommands {
    /// Transfer funds
    #[structopt(name = "transfer")]
    Transfer {
        /// Sending address (must be one of our accounts)
        #[structopt(long)]
        from: String,

        /// Recipient address
        #[structopt(long)]
        to: String,

        /// Object to transfer, in 20 bytes Hex string
        object_id: String,
    },

    /// Obtain the spendable balance
    #[structopt(name = "query_balance")]
    QueryBalance {
        /// Address of the account
        address: String,
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

    /// Create new user accounts and print the public keys
    #[structopt(name = "create_accounts")]
    CreateAccounts {
        /// known initial balance of the account
        #[structopt(long)]
        initial_objects: Option<Vec<String>>,

        /// Number of additional accounts to create
        num: u32,
    },
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = ClientOpt::from_args();

    let send_timeout = Duration::from_micros(options.send_timeout);
    let recv_timeout = Duration::from_micros(options.recv_timeout);
    let accounts_config_path = &options.accounts;
    let committee_config_path = &options.committee;
    let buffer_size = options.buffer_size;

    let mut accounts_config =
        AccountsConfig::read_or_create(accounts_config_path).expect("Unable to read user accounts");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Unable to read committee config file");

    match options.cmd {
        ClientCommands::Transfer {
            from,
            to,
            object_id,
        } => {
            let sender = decode_address(&from).expect("Failed to decode sender's address");
            let recipient = decode_address(&to).expect("Failed to decode recipient's address");
            let object_id = decode_object_id(&object_id).unwrap();

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
                    .transfer_to_fastpay(object_id, recipient, UserData::default())
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

        ClientCommands::QueryBalance { address } => {
            let user_address = decode_address(&address).expect("Failed to decode address");

            let mut rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let mut client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    user_address,
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

        ClientCommands::Benchmark {
            max_in_flight,
            max_orders,
            server_configs,
        } => {
            let max_orders = max_orders.unwrap_or_else(|| accounts_config.num_accounts());

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
                    warn!("Using server configs provided by --server-configs");
                    let files = files.iter().map(AsRef::as_ref).collect();
                    make_benchmark_certificates_from_orders_and_server_configs(orders, files)
                } else {
                    warn!("Using committee config");
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
                                confirmed.insert(info.object_id);
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

        ClientCommands::CreateAccounts {
            initial_objects,
            num,
        } => {
            let num_accounts: u32 = num;
            let object_ids = match initial_objects {
                Some(object_ids) => {
                    object_ids.into_iter().map(|string| decode_object_id(&string).unwrap()).collect()
                }
                None => Vec::new()
            };

            for _ in 0..num_accounts {
                let account = UserAccount::new(object_ids.clone());
                println!("{}:{:?}", encode_address(&account.address), object_ids);
                accounts_config.insert(account);
            }
            accounts_config
                .write(accounts_config_path)
                .expect("Unable to write user accounts");
        }
    }
}
