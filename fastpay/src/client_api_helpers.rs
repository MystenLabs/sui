// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use fastpay::{
    config::{AccountsConfig, AuthorityServerConfig, CommitteeConfig},
    network,
};

use fastpay_core::client::*;
use fastx_types::{base_types::*, committee::Committee, messages::*, serialize::*};

use bytes::Bytes;
use futures::stream::StreamExt;
use log::*;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

pub fn make_authority_clients(
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
            buffer_size,
            send_timeout,
            recv_timeout,
        );
        authority_clients.insert(config.address, client);
    }
    authority_clients
}

pub fn make_authority_mass_clients(
    committee_config: &CommitteeConfig,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    max_in_flight: u64,
) -> Vec<network::MassClient> {
    let mut authority_clients = Vec::new();
    for config in &committee_config.authorities {
        let client = network::MassClient::new(
            config.network_protocol,
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

pub fn make_client_state(
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
pub fn make_benchmark_transfer_orders(
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
pub fn make_benchmark_certificates_from_orders_and_server_configs(
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
pub fn make_benchmark_certificates_from_votes(
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
pub(crate) async fn mass_broadcast_orders(
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

pub fn mass_update_recipients(
    accounts_config: &mut AccountsConfig,
    certificates: Vec<(ObjectID, Bytes)>,
) {
    for (_object_id, buf) in certificates {
        if let Ok(SerializedMessage::Cert(certificate)) = deserialize_message(&buf[..]) {
            accounts_config.update_for_received_transfer(*certificate);
        }
    }
}

pub fn deserialize_response(response: &[u8]) -> Option<ObjectInfoResponse> {
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
pub fn find_cached_owner_by_object_id(
    account_config: &AccountsConfig,
    object_id: ObjectID,
) -> Option<&PublicKeyBytes> {
    account_config
        .find_account(&object_id)
        .map(|acc| &acc.address)
}
