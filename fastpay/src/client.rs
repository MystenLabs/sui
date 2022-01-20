// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use fastpay::{config::*, network, transport};
use fastpay_core::client::*;
use fastx_types::{base_types::*, committee::Committee, messages::*, serialize::*};
use move_core_types::transaction_argument::convert_txn_args;

use bytes::Bytes;
use fastx_types::object::Object;
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
            config.host,
            config.base_port,
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
) -> Vec<network::MassClient> {
    let mut authority_clients = Vec::new();
    for config in &committee_config.authorities {
        let client = network::MassClient::new(
            config.host.clone(),
            config.base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
            max_in_flight,
        );
        authority_clients.push(client);
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
        account.certificates.clone(),
        account.object_refs.clone(),
    )
}

/// Make one transfer order per account, up to `max_orders` transfers.
fn make_benchmark_transfer_orders(
    accounts_config: &mut AccountsConfig,
    max_orders: usize,
) -> (Vec<Order>, Vec<(ObjectID, Bytes)>) {
    let mut orders = Vec::new();
    let mut serialized_orders = Vec::new();
    // TODO: deterministic sequence of orders to recover from interrupted benchmarks.
    let mut next_recipient = get_key_pair().0;
    for account in accounts_config.accounts_mut() {
        let gas_object_id = *account.gas_object_ids.iter().next().unwrap();
        let gas_object_ref = *account.object_refs.get(&gas_object_id).unwrap();
        let (gas_object_id, _, _) = gas_object_ref;
        let object_ref = *account
            .object_refs
            .iter()
            .find_map(|(key, object_ref)| {
                if *key != gas_object_id {
                    Some(object_ref)
                } else {
                    None
                }
            })
            .unwrap();
        let transfer = Transfer {
            object_ref,
            sender: account.address,
            recipient: Address::FastPay(next_recipient),
            gas_payment: gas_object_ref,
        };
        debug!("Preparing transfer order: {:?}", transfer);
        let (object_id, seq, digest) = object_ref;
        account
            .object_refs
            .insert(object_id, (object_id, seq.increment(), digest));
        next_recipient = account.address;
        let order = Order::new_transfer(transfer.clone(), &account.key);
        orders.push(order.clone());
        let serialized_order = serialize_order(&order);
        serialized_orders.push((object_id, serialized_order.into()));
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
) -> Vec<(ObjectID, Bytes)> {
    let mut keys = Vec::new();
    for file in server_config {
        let server_config = AuthorityServerConfig::read(file).expect("Fail to read server config");
        keys.push((server_config.authority.address, server_config.key));
    }
    let committee = Committee::new(keys.iter().map(|(k, _)| (*k, 1)).collect());
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
        serialized_certificates.push((*order.object_id(), serialized_certificate.into()));
    }
    serialized_certificates
}

/// Try to aggregate votes into certificates.
fn make_benchmark_certificates_from_votes(
    committee_config: &CommitteeConfig,
    votes: Vec<SignedOrder>,
) -> Vec<(ObjectID, Bytes)> {
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
                certificates.push((object_id, buf.into()));
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
    orders: Vec<(ObjectID, Bytes)>,
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
    for client in authority_clients {
        let mut requests = Vec::new();
        for (_object_id, buf) in &orders {
            requests.push(buf.clone());
        }
        streams.push(client.run(requests, 1));
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
    certificates: Vec<(ObjectID, Bytes)>,
) {
    for (_object_id, buf) in certificates {
        if let Ok(SerializedMessage::Cert(certificate)) = deserialize_message(&buf[..]) {
            accounts_config.update_for_received_transfer(*certificate);
        }
    }
}

fn deserialize_response(response: &[u8]) -> Option<ObjectInfoResponse> {
    match deserialize_message(response) {
        Ok(SerializedMessage::ObjectInfoResp(info)) => Some(*info),
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
fn find_cached_owner_by_object_id(
    account_config: &AccountsConfig,
    object_id: ObjectID,
) -> Option<&PublicKeyBytes> {
    account_config
        .find_account(&object_id)
        .map(|acc| &acc.address)
}

fn show_object_effects(order_effects: OrderEffects) {
    if !order_effects.mutated.is_empty() {
        println!("Mutated Objects:");
        for (obj, _) in order_effects.mutated {
            println!("{:?} {:?} {:?}", obj.0, obj.1, obj.2);
        }
    }
    if !order_effects.deleted.is_empty() {
        println!("Deleted Objects:");
        for obj in order_effects.deleted {
            println!("{:?} {:?} {:?}", obj.0, obj.1, obj.2);
        }
    }
}

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

    let mut accounts_config =
        AccountsConfig::read_or_create(accounts_config_path).expect("Unable to read user accounts");
    let committee_config =
        CommitteeConfig::read(committee_config_path).expect("Unable to read committee config file");

    match options.cmd {
        ClientCommands::GetObjInfo { obj_id } => {
            // Pick the first (or any) account for use in finding obj info
            let account = accounts_config
                .nth_account(0)
                .expect("Account config is invalid")
                .address;
            // Fetch the object ref
            let mut client_state = make_client_state(
                &accounts_config,
                &committee_config,
                account,
                buffer_size,
                send_timeout,
                recv_timeout,
            );
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                // Fetch the object info for the object
                let obj_info_req = ObjectInfoRequest {
                    object_id: obj_id,
                    request_sequence_number: None,
                    request_received_transfers_excluding_first_nth: None,
                };
                let obj_info = client_state.get_object_info(obj_info_req).await.unwrap();
                println!("Owner: {:#?}", obj_info.object.owner);
                println!("Version: {:#?}", obj_info.object.version().value());
                println!("ID: {:#?}", obj_info.object.id());
                println!("Readonly: {:#?}", obj_info.object.is_read_only());
                println!(
                    "Type: {:#?}",
                    obj_info
                        .object
                        .data
                        .type_()
                        .map_or("Type Unwrap Failed".to_owned(), |type_| type_
                            .module
                            .as_ident_str()
                            .to_string())
                );
            });
        }

        ClientCommands::Call { path } => {
            let config = MoveCallConfig::read(&path).unwrap();
            // Find owner of gas object
            let owner = find_cached_owner_by_object_id(&accounts_config, config.gas_object_id)
                .expect("Cannot find owner for gas object");

            let mut client_state = make_client_state(
                &accounts_config,
                &committee_config,
                *owner,
                buffer_size,
                send_timeout,
                recv_timeout,
            );

            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                // Fetch the object info for the package
                let package_obj_info_req = ObjectInfoRequest {
                    object_id: config.package_obj_id,
                    request_sequence_number: None,
                    request_received_transfers_excluding_first_nth: None,
                };
                let package_obj_info = client_state
                    .get_object_info(package_obj_info_req)
                    .await
                    .unwrap();
                let package_obj_ref = package_obj_info.object.to_object_reference();

                // Fetch the object info for the gas obj
                let gas_obj_info_req = ObjectInfoRequest {
                    object_id: config.gas_object_id,
                    request_sequence_number: None,
                    request_received_transfers_excluding_first_nth: None,
                };

                let gas_obj_info = client_state
                    .get_object_info(gas_obj_info_req)
                    .await
                    .unwrap();
                let gas_obj_ref = gas_obj_info.object.to_object_reference();

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in config.object_args_ids {
                    // Fetch the obj ref
                    let obj_info_req = ObjectInfoRequest {
                        object_id: obj_id,
                        request_sequence_number: None,
                        request_received_transfers_excluding_first_nth: None,
                    };

                    let obj_info = client_state.get_object_info(obj_info_req).await.unwrap();
                    object_args_refs.push(obj_info.object.to_object_reference());
                }

                let pure_args = convert_txn_args(&config.pure_args);

                let call_ret = client_state
                    .move_call(
                        package_obj_ref,
                        config.module,
                        config.function,
                        config.type_args,
                        gas_obj_ref,
                        object_args_refs,
                        pure_args,
                        config.gas_budget,
                    )
                    .await
                    .unwrap();
                println!("Cert: {:?}", call_ret.0);
                show_object_effects(call_ret.1);
            });
        }

        ClientCommands::Transfer {
            to,
            object_id,
            gas_object_id,
        } => {
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let owner = find_cached_owner_by_object_id(&accounts_config, gas_object_id)
                    .expect("Cannot find owner for gas object");
                let mut client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    *owner,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );
                info!("Starting transfer");
                let time_start = Instant::now();
                let cert = client_state
                    .transfer_object(object_id, gas_object_id, to)
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
                    to,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );
                recipient_client_state.receive_object(cert).await.unwrap();
                accounts_config.update_from_state(&recipient_client_state);
                accounts_config
                    .write(accounts_config_path)
                    .expect("Unable to write user accounts");
                info!("Saved user account states");
            });
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
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let client_state = make_client_state(
                    &accounts_config,
                    &committee_config,
                    address,
                    buffer_size,
                    send_timeout,
                    recv_timeout,
                );

                let object_refs = client_state.object_refs();

                accounts_config.update_from_state(&client_state);
                accounts_config
                    .write(accounts_config_path)
                    .expect("Unable to write user accounts");

                for (obj_id, object_ref) in object_refs {
                    println!("{}: {:?}", obj_id, object_ref);
                }
            });
        }

        ClientCommands::Benchmark {
            max_in_flight,
            max_orders,
            server_configs,
        } => {
            let max_orders = max_orders.unwrap_or_else(|| accounts_config.num_accounts());

            let rt = Runtime::new().unwrap();
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
                info!("Received {} valid votes.", votes.len());

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
                                confirmed.insert(info.object.id());
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
            num,
            gas_objs_per_account,
            // TODO: Integrate gas logic with https://github.com/MystenLabs/fastnft/pull/97
            value_per_per_obj: _,
            initial_state_config_path,
        } => {
            let num_accounts: u32 = num;
            let mut init_state_cfg: InitialStateConfig = InitialStateConfig::new();

            for _ in 0..num_accounts {
                let mut object_refs = Vec::new();

                for _ in 0..gas_objs_per_account {
                    let object = Object::with_id_owner_gas_for_testing(
                        ObjectID::random(),
                        SequenceNumber::default(),
                        FastPayAddress::random_for_testing_only(),
                        100000,
                    );
                    object_refs.push(object.to_object_reference());
                }

                let gas_object_ids = object_refs.iter().map(|(id, _, _)| *id).collect();
                let account = UserAccount::new(object_refs.clone(), gas_object_ids);

                init_state_cfg.config.push(InitialStateConfigEntry {
                    address: account.address,
                    object_refs,
                });

                accounts_config.insert(account);
            }
            init_state_cfg
                .write(initial_state_config_path.as_str())
                .expect("Unable to write to initial state config file");
            accounts_config
                .write(accounts_config_path)
                .expect("Unable to write user accounts");
        }
    }
}
