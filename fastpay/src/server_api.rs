// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastpay::{config::*, network};
use fastpay_core::authority::*;
use fastx_types::{base_types::*, committee::Committee, object::Object};

use futures::future::join_all;
use log::*;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Create the configs for a server running one FastX authority
pub fn create_server_configs(
    host: String,
    port: u32,
    database_path: String,
) -> AuthorityServerConfig {
    // Create a keypair for this server
    let (address, key) = get_key_pair();
    let authority = AuthorityConfig {
        address,
        host,
        base_port: port,
        database_path,
    };
    AuthorityServerConfig { authority, key }
}

/// Start the configs for a server running one FastX authority
#[allow(clippy::too_many_arguments)]
pub fn run_server(
    local_ip_addr: &str,
    server_config: AuthorityServerConfig,
    committee_config: CommitteeConfig,
    initial_accounts_config: InitialStateConfig,
    buffer_size: usize,
) {
    let committee = Committee::new(committee_config.voting_rights());

    let store = Arc::new(AuthorityStore::open(
        Path::new(&server_config.authority.database_path),
        None,
    ));

    // Load initial states
    let rt = Runtime::new().unwrap();

    let state = rt.block_on(async {
        let state = AuthorityState::new_with_genesis_modules(
            committee,
            server_config.authority.address,
            server_config.key.copy(),
            store,
        )
        .await;
        for initial_state_cfg_entry in &initial_accounts_config.config {
            let address = &initial_state_cfg_entry.address;
            println!("{}", initial_state_cfg_entry.object_ids_and_gas_vals.len());
            for (object_id, gas_val) in &initial_state_cfg_entry.object_ids_and_gas_vals {
                let object = Object::with_id_owner_gas_for_testing(
                    *object_id,
                    SequenceNumber::new(),
                    *address,
                    *gas_val,
                );
                state.init_order_lock(object.to_object_reference()).await;
                state.insert_object(object).await;
            }
        }
        state
    });

    let server = network::Server::new(
        local_ip_addr.to_string(),
        server_config.authority.base_port,
        state,
        buffer_size,
    );

    let rt = Runtime::new().unwrap();
    let mut handles = Vec::new();

    handles.push(async move {
        let spawned_server = match server.spawn().await {
            Ok(server) => server,
            Err(err) => {
                error!("Failed to start server: {}", err);
                return;
            }
        };
        if let Err(err) = spawned_server.join().await {
            error!("Server ended with an error: {}", err);
        }
    });

    rt.block_on(join_all(handles));
}
