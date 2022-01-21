// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::client_api_helpers::{self, *};
use fastpay::config::*;
use fastpay_core::client::Client;
use fastx_types::{base_types::*, messages::*};
use move_core_types::{account_address::AccountAddress, transaction_argument::convert_txn_args};

use log::*;
use std::collections::{BTreeMap, HashSet};
use tokio::runtime::Runtime;

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
) -> BTreeMap<AccountAddress, SequenceNumber> {
    let rt = Runtime::new().unwrap();
    rt.block_on(async move {
        let mut client_state = client_api_helpers::make_client_state(
            accounts_config,
            committee_config,
            address,
            buffer_size,
            send_timeout,
            recv_timeout,
        );

        // Sync with high prio
        for _ in 0..committee_config.authorities.len() {
            client_state.sync_client_state_with_random_authority();
        }
        let objects_ids = client_state.object_ids();
        objects_ids.clone()
    })
}

/// Transfer to a diff addr
pub fn transfer_object(
    to: FastPayAddress,
    from: FastPayAddress,
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

        println!("{:#?}", client_state.all_certificates());

        accounts_config.update_from_state(&client_state);
        //info!("Updating recipient's local balance");
        let mut recipient_client_state = client_api_helpers::make_client_state(
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
        println!("{:#?}", client_state.all_certificates());

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
    let mut client_state = client_api_helpers::make_client_state(
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
            client_api_helpers::make_benchmark_transfer_orders(accounts_config, max_orders);
        let responses = client_api_helpers::mass_broadcast_orders(
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
                client_api_helpers::deserialize_response(&buf[..])
                    .and_then(|info| info.pending_confirmation)
            })
            .collect();
        info!("Received {} valid votes.", votes.len());

        warn!("Starting benchmark phase 2 (confirmation orders)");
        let certificates = if let Some(files) = server_configs {
            warn!("Using server configs provided by --server-configs");
            let files = files.iter().map(AsRef::as_ref).collect();
            client_api_helpers::make_benchmark_certificates_from_orders_and_server_configs(
                orders, files,
            )
        } else {
            warn!("Using committee config");
            client_api_helpers::make_benchmark_certificates_from_votes(committee_config, votes)
        };
        let responses = client_api_helpers::mass_broadcast_orders(
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
        client_api_helpers::mass_update_recipients(accounts_config, certificates);
    });
}
