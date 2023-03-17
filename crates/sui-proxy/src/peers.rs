// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{bail, Context, Result};
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::traits::ToFromBytes;
use multiaddr::Multiaddr;
use serde::Deserialize;
use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use sui_tls::Allower;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use tracing::{debug, error, info};

/// SuiNods a mapping of public key to SuiPeer data
pub type SuiPeers = Arc<RwLock<HashMap<Ed25519PublicKey, SuiPeer>>>;

/// A SuiPeer is the collated sui chain data we have about validators
#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct SuiPeer {
    pub name: String,
    pub p2p_address: Multiaddr,
    pub public_key: Ed25519PublicKey,
}

/// SuiNodeProvider queries the sui blockchain and keeps a record of known validators based on the response from
/// sui_getValidators.  The node name, public key and other info is extracted from the chain and stored in this
/// data structure.  We pass this struct to the tls verifier and it depends on the state contained within.
/// Handlers also use this data in an Extractor extension to check incoming clients on the http api against known keys.
#[derive(Debug, Clone)]
pub struct SuiNodeProvider {
    nodes: SuiPeers,
    rpc_url: String,
    rpc_poll_interval: Duration,
}

impl Allower for SuiNodeProvider {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        self.nodes.read().unwrap().contains_key(key)
    }
}

impl SuiNodeProvider {
    pub fn new(rpc_url: String, rpc_poll_interval: Duration) -> Self {
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        Self {
            nodes,
            rpc_url,
            rpc_poll_interval,
        }
    }

    /// get is used to retrieve peer info in our handlers
    pub fn get(&self, key: &Ed25519PublicKey) -> Option<SuiPeer> {
        debug!("look for {:?}", key);
        if let Some(v) = self.nodes.read().unwrap().get(key) {
            return Some(SuiPeer {
                name: v.name.to_owned(),
                p2p_address: v.p2p_address.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        None
    }
    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &SuiPeers {
        &self.nodes
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut SuiPeers {
        &mut self.nodes
    }

    /// get_validators will retrieve known validators
    async fn get_validators(url: String) -> Result<SuiSystemStateSummary> {
        let client = reqwest::Client::builder().build().unwrap();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method":"sui_getLatestSuiSystemState",
            "id":1,
        });
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.to_string())
            .send()
            .await
            .context("unable to perform rpc")?;

        #[derive(Debug, Deserialize)]
        struct ResponseBody {
            result: SuiSystemStateSummary,
        }

        let body = match response.json::<ResponseBody>().await {
            Ok(b) => b,
            Err(error) => {
                bail!("unable to decode json: {error}")
            }
        };

        Ok(body.result)
    }

    /// poll_peer_list will act as a refresh interval for our cache
    pub fn poll_peer_list(&self) {
        info!("Started polling for peers using rpc: {}", self.rpc_url);

        let rpc_poll_interval = self.rpc_poll_interval;
        let rpc_url = self.rpc_url.to_owned();
        let nodes = self.nodes.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(rpc_poll_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                match Self::get_validators(rpc_url.to_owned()).await {
                    Ok(summary) => {
                        let peers = extract(summary);
                        // maintain the tls acceptor set
                        let mut allow = nodes.write().unwrap();
                        allow.clear();
                        allow.extend(peers);
                        info!("{} peers managed to make it on the allow list", allow.len());
                    }
                    Err(error) => error!("unable to refresh peer list: {error}"),
                }
            }
        });
    }
}

/// extract will get the network pubkey bytes from a SuiValidatorSummary type.  This type comes from a
/// full node rpc result.  See get_validators for details.  The key here, if extracted successfully, will
/// ultimately be stored in the allow list and let us communicate with those actual peers via tls.
fn extract(summary: SuiSystemStateSummary) -> impl Iterator<Item = (Ed25519PublicKey, SuiPeer)> {
    summary.active_validators.into_iter().filter_map(|vm| {
        match Ed25519PublicKey::from_bytes(&vm.network_pubkey_bytes) {
            Ok(public_key) => {
                let Ok(p2p_address) = Multiaddr::try_from(vm.p2p_address) else {
                    error!("refusing to add peer to allow list; unable to decode multiaddr for {}", vm.name);
                    return None // scoped to filter_map
                };
                debug!("adding public key {:?} for address {:?}", public_key, p2p_address);
                Some((public_key.clone(), SuiPeer { name: vm.name, p2p_address, public_key })) // scoped to filter_map
            },
            Err(error) => {
                error!(
                "unable to decode public key for name: {:?} sui_address: {:?} error: {error}",
                vm.name, vm.sui_address);
                 None  // scoped to filter_map
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{generate_self_cert, CertKeyPair};
    use serde::Serialize;
    use sui_types::{
        base_types::SuiAddress,
        id::ID,
        sui_system_state::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
        SUI_SYSTEM_STATE_OBJECT_ID,
    };

    /// creates a test that binds our proxy use case to the structure in sui_getLatestSuiSystemState
    /// most of the fields are garbage, but we will send the results of the serde process to a private decode
    /// function that should always work if the structure is valid for our use
    #[test]
    fn depend_on_sui_sui_system_state_summary() {
        let CertKeyPair(_, client_pub_key) = generate_self_cert("sui".into());
        let p2p_address: Multiaddr = "/ip4/127.0.0.1/tcp/10000"
            .parse()
            .expect("expected a multiaddr value");
        // all fields here just satisfy the field types, with exception to active_validators, we use
        // some of those.
        let depends_on = SuiSystemStateSummary {
            epoch: 1,
            protocol_version: 2,
            system_state_version: 3,
            storage_fund: 4,
            reference_gas_price: 5,
            safe_mode: false,
            epoch_start_timestamp_ms: 123456,
            governance_start_epoch: 123456,
            epoch_duration_ms: 123456789,
            stake_subsidy_epoch_counter: 1,
            stake_subsidy_balance: 1,
            stake_subsidy_current_epoch_amount: 1,
            total_stake: 1,
            active_validators: vec![SuiValidatorSummary {
                sui_address: SuiAddress::random_for_testing_only(),
                protocol_pubkey_bytes: vec![],
                network_pubkey_bytes: Vec::from(client_pub_key.as_bytes()),
                worker_pubkey_bytes: vec![],
                proof_of_possession_bytes: vec![],
                name: "fooman-validator".into(),
                description: "empty".into(),
                image_url: "empty".into(),
                project_url: "empty".into(),
                net_address: "empty".into(),
                p2p_address: format!("{p2p_address}"),
                primary_address: "empty".into(),
                worker_address: "empty".into(),
                next_epoch_protocol_pubkey_bytes: None,
                next_epoch_proof_of_possession: None,
                next_epoch_network_pubkey_bytes: None,
                next_epoch_worker_pubkey_bytes: None,
                next_epoch_net_address: None,
                next_epoch_p2p_address: None,
                next_epoch_primary_address: None,
                next_epoch_worker_address: None,
                voting_power: 1,
                operation_cap_id: ID::new(SUI_SYSTEM_STATE_OBJECT_ID),
                gas_price: 1,
                commission_rate: 1,
                next_epoch_stake: 1,
                next_epoch_gas_price: 1,
                next_epoch_commission_rate: 1,
                staking_pool_id: SUI_SYSTEM_STATE_OBJECT_ID,
                staking_pool_activation_epoch: None,
                staking_pool_deactivation_epoch: None,
                staking_pool_sui_balance: 1,
                rewards_pool: 1,
                pool_token_balance: 1,
                pending_stake: 1,
                pending_total_sui_withdraw: 1,
                pending_pool_token_withdraw: 1,
                exchange_rates_id: SUI_SYSTEM_STATE_OBJECT_ID,
                exchange_rates_size: 1,
            }],
            pending_active_validators_id: SUI_SYSTEM_STATE_OBJECT_ID,
            pending_active_validators_size: 1,
            pending_removals: vec![],
            staking_pool_mappings_id: SUI_SYSTEM_STATE_OBJECT_ID,
            staking_pool_mappings_size: 1,
            inactive_pools_id: SUI_SYSTEM_STATE_OBJECT_ID,
            inactive_pools_size: 1,
            validator_candidates_id: SUI_SYSTEM_STATE_OBJECT_ID,
            validator_candidates_size: 1,
            at_risk_validators: vec![],
            validator_report_records: vec![],
        };

        #[derive(Debug, Serialize, Deserialize)]
        struct ResponseBody {
            result: SuiSystemStateSummary,
        }

        let r = serde_json::to_string(&ResponseBody { result: depends_on })
            .expect("expected to serialize ResponseBody{SuiSystemStateSummary}");

        let deserialized = serde_json::from_str::<ResponseBody>(&r)
            .expect("expected to deserialize ResponseBody{SuiSystemStateSummary}");

        let peers = extract(deserialized.result);
        assert_eq!(peers.count(), 1, "peers should have been a length of 1");
    }
}
