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
