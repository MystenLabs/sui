// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

//use crate::{self, *};
use fastpay::config::*;
//use fastpay_core::client::Client;
use fastx_types::{
    base_types::*,
    messages::*,
    serialize::{deserialize_message, SerializedMessage},
};
use futures::StreamExt;
use move_core_types::{account_address::AccountAddress, transaction_argument::convert_txn_args};

use log::*;
use std::collections::HashSet;
use tokio::runtime::Runtime;

use fastpay::{
    config::{AccountsConfig, AuthorityServerConfig, CommitteeConfig},
    network,
};

use fastpay_core::client::*;
use fastx_types::{committee::Committee, serialize::*};

use bytes::Bytes;

use std::{collections::HashMap, time::Instant};

/// Creates the configs to be used by an account with some starting objects
pub fn create_account_configs(
    accounts_config: &mut AccountsConfig,
    num_accounts: usize,
    value_per_per_obj: u32,
    gas_objs_per_account: u32,
) -> InitialStateConfig {
    let mut init_state_cfg: InitialStateConfig = InitialStateConfig::new();

    for _ in 0..num_accounts {
        let mut obj_ids = Vec::new();
        let mut obj_ids_gas = Vec::new();

        for _ in 0..gas_objs_per_account {
            let id = ObjectID::random();
            obj_ids_gas.push((id, value_per_per_obj as u64));
            obj_ids.push(id);
        }

        let account = UserAccount::new(obj_ids.clone(), obj_ids.clone());

        init_state_cfg.config.push(InitialStateConfigEntry {
            address: account.address,
            object_ids_and_gas_vals: obj_ids_gas,
        });

        accounts_config.insert(account);
    }
    init_state_cfg
}

/// Retrives the objects for this acc
pub fn get_account_objects(
    address: FastPayAddress,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    buffer_size: usize,
    accounts_config: &mut AccountsConfig,
    committee_config: &CommitteeConfig,
) -> Vec<(AccountAddress, SequenceNumber)> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        let mut client_state = make_client_state(
            accounts_config,
            committee_config,
            address,
            buffer_size,
            send_timeout,
            recv_timeout,
        );

        // Sync with high prio
        for _ in 0..committee_config.authorities.len() {
            let _ = client_state.sync_client_state_with_random_authority();
        }
        let objects_ids = client_state
            .object_ids()
            .iter()
            .map(|e| (*e.0, *e.1))
            .collect::<Vec<(_, _)>>();

        objects_ids
    })
}

/// Transfer to a diff addr
pub fn transfer_object(
    from: FastPayAddress,
    to: FastPayAddress,
    object_id: ObjectID,
    gas_object_id: ObjectID,
    accounts_config: &mut AccountsConfig,
    committee_config: &CommitteeConfig,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    buffer_size: usize,
) -> CertifiedOrder {
    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        let mut client_state = make_client_state(
            accounts_config,
            committee_config,
            from,
            buffer_size,
            send_timeout,
            recv_timeout,
        );
        let cert = client_state
            .transfer_object(object_id, gas_object_id, to)
            .await
            .unwrap();
        accounts_config.update_from_state(&client_state);
        let mut recipient_client_state = make_client_state(
            accounts_config,
            committee_config,
            to,
            buffer_size,
            send_timeout,
            recv_timeout,
        );
        recipient_client_state
            .receive_object(cert.clone())
            .await
            .unwrap();
        client_state.all_certificates();

        accounts_config.update_from_state(&recipient_client_state);
        cert
    })
}

/// Get the object info for addr
pub fn get_object_info(
    obj_id: ObjectID,
    accounts_config: &mut AccountsConfig,
    committee_config: &CommitteeConfig,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    buffer_size: usize,
) -> ObjectInfoResponse {
    // Pick the first (or any) account for use in finding obj info
    let account = accounts_config
        .nth_account(0)
        .expect("Account config is invalid")
        .address;
    // Fetch the object ref
    let mut client_state = make_client_state(
        accounts_config,
        committee_config,
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
        obj_info
    })
}

/// Execute Move Call with given move cfg
pub fn move_call(
    config: MoveCallConfig,
    accounts_config: &mut AccountsConfig,
    committee_config: &CommitteeConfig,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    buffer_size: usize,
) -> (CertifiedOrder, OrderEffects) {
    let owner = find_cached_owner_by_object_id(accounts_config, config.gas_object_id)
        .expect("Cannot find owner for gas object");
    let mut client_state = make_client_state(
        accounts_config,
        committee_config,
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
        call_ret
    })
}

/// Run a benchmark on client
pub fn benchmark(
    accounts_config: &mut AccountsConfig,
    committee_config: &CommitteeConfig,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    buffer_size: usize,
    max_in_flight: u64,
    max_orders: Option<usize>,
    server_configs: Option<Vec<String>>,
) {
    let max_orders = max_orders.unwrap_or_else(|| accounts_config.num_accounts());

    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        warn!("Starting benchmark phase 1 (transfer orders)");
        let (orders, serialize_orders) =
            make_benchmark_transfer_orders(accounts_config, max_orders);
        let responses = mass_broadcast_orders(
            "transfer",
            committee_config,
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
            make_benchmark_certificates_from_votes(committee_config, votes)
        };
        let responses = mass_broadcast_orders(
            "confirmation",
            committee_config,
            buffer_size,
            send_timeout,
            recv_timeout,
            max_in_flight,
            certificates.clone(),
        )
        .await;
        let mut confirmed = HashSet::new();
        let num_valid = responses
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
        mass_update_recipients(accounts_config, certificates);
    });
}

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
        account.object_ids.clone(),
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
        let gas_object_seq = *account.object_ids.get(&gas_object_id).unwrap();
        let object_id = *account
            .object_ids
            .keys()
            .find(|key| *key != &gas_object_id)
            .unwrap();
        let transfer = Transfer {
            object_ref: (
                object_id,
                account.object_ids[&object_id],
                // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                ObjectDigest::new([0; 32]),
            ),
            sender: account.address,
            recipient: Address::FastPay(next_recipient),
            gas_payment: (
                gas_object_id,
                gas_object_seq,
                // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                ObjectDigest::new([0; 32]),
            ),
        };
        debug!("Preparing transfer order: {:?}", transfer);
        account
            .object_ids
            .insert(object_id, account.object_ids[&object_id].increment());
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

/// Broadcast a bulk of requests to each authority.pub
pub async fn mass_broadcast_orders(
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
