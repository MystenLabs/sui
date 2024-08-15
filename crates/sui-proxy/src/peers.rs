// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{bail, Context, Result};
use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PublicKey};
use fastcrypto::traits::{KeyPair, ToFromBytes};
use futures::stream::{self, StreamExt};
use multiaddr::Multiaddr;
use once_cell::sync::Lazy;
use prometheus::{register_counter_vec, register_histogram_vec};
use prometheus::{CounterVec, HistogramVec};
use rustls::crypto::hmac::Key;
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use sui_tls::Allower;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeSummary;
use sui_types::crypto::EncodeDecodeBase64;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use tracing::{debug, error, info};

static JSON_RPC_STATE: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "json_rpc_state",
        "Number of successful/failed requests made.",
        &["rpc_method", "status"]
    )
    .unwrap()
});
static JSON_RPC_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "json_rpc_duration_seconds",
        "The json-rpc latencies in seconds.",
        &["rpc_method"],
        vec![
            0.0008, 0.0016, 0.0032, 0.0064, 0.0128, 0.0256, 0.0512, 0.1024, 0.2048, 0.4096, 0.8192,
            1.0, 1.25, 1.5, 1.75, 2.0, 4.0, 8.0
        ],
    )
    .unwrap()
});

/// SuiNods a mapping of public key to SuiPeer data
pub type SuiPeers = Arc<RwLock<HashMap<Ed25519PublicKey, SuiPeer>>>;
pub type BridgePeers = Arc<RwLock<HashMap<Ed25519PublicKey, BridgePeer>>>;

/// A SuiPeer is the collated sui chain data we have about validators
#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct SuiPeer {
    pub name: String,
    pub p2p_address: Multiaddr,
    pub public_key: Ed25519PublicKey,
}
/// A BridgePeer is the collated sui chain data we have about validators
#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct BridgePeer {
    pub sui_address: SuiAddress,
    pub public_key: Ed25519PublicKey,
}

/// SuiNodeProvider queries the sui blockchain and keeps a record of known validators based on the response from
/// sui_getValidators.  The node name, public key and other info is extracted from the chain and stored in this
/// data structure.  We pass this struct to the tls verifier and it depends on the state contained within.
/// Handlers also use this data in an Extractor extension to check incoming clients on the http api against known keys.
#[derive(Debug, Clone)]
pub struct SuiNodeProvider {
    nodes: SuiPeers,
    bridge_nodes: BridgePeers,
    static_nodes: SuiPeers,
    rpc_url: String,
    rpc_poll_interval: Duration,
}

impl Allower for SuiNodeProvider {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        self.static_nodes.read().unwrap().contains_key(key)
            || self.nodes.read().unwrap().contains_key(key)
            || self.bridge_nodes.read().unwrap().contains_key(key)
    }
}

impl SuiNodeProvider {
    pub fn new(rpc_url: String, rpc_poll_interval: Duration, static_peers: Vec<SuiPeer>) -> Self {
        // build our hashmap with the static pub keys. we only do this one time at binary startup.
        let static_nodes: HashMap<Ed25519PublicKey, SuiPeer> = static_peers
            .into_iter()
            .map(|v| (v.public_key.clone(), v))
            .collect();
        let static_nodes = Arc::new(RwLock::new(static_nodes));
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        let bridge_nodes = Arc::new(RwLock::new(HashMap::new()));
        Self {
            nodes,
            bridge_nodes,
            static_nodes,
            rpc_url,
            rpc_poll_interval,
        }
    }

    /// get is used to retrieve peer info in our handlers
    pub fn get(&self, key: &Ed25519PublicKey) -> Option<SuiPeer> {
        debug!("look for {:?}", key);
        // check static nodes first
        if let Some(v) = self.static_nodes.read().unwrap().get(key) {
            return Some(SuiPeer {
                name: v.name.to_owned(),
                p2p_address: v.p2p_address.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        // check dynamic nodes
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

    /// get_bridge_validators will retrieve known bridge validators
    async fn get_bridge_validators(url: String) -> Result<BridgeSummary> {
        let rpc_method = "suix_getLatestBridge";
        let observe = || {
            let timer = JSON_RPC_DURATION
                .with_label_values(&[rpc_method])
                .start_timer();
            || {
                timer.observe_duration();
            }
        }();
        let client = reqwest::Client::builder().build().unwrap();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method":rpc_method,
            "id":1,
        });
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.to_string())
            .send()
            .await
            .with_context(|| {
                JSON_RPC_STATE
                    .with_label_values(&[rpc_method, "failed_get"])
                    .inc();
                observe();
                "unable to perform json rpc"
            })?;

        let raw = response.bytes().await.with_context(|| {
            JSON_RPC_STATE
                .with_label_values(&[rpc_method, "failed_body_extract"])
                .inc();
            observe();
            "unable to extract body bytes from json rpc"
        })?;

        #[derive(Debug, Deserialize)]
        struct ResponseBody {
            result: BridgeSummary,
        }

        let body: ResponseBody = match serde_json::from_slice(&raw) {
            Ok(b) => b,
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&[rpc_method, "failed_json_decode"])
                    .inc();
                observe();
                bail!(
                    "unable to decode json: {error} response from json rpc: {:?}",
                    raw
                )
            }
        };
        JSON_RPC_STATE
            .with_label_values(&[rpc_method, "success"])
            .inc();
        observe();
        Ok(body.result)
    }

    /// get_validators will retrieve known validators
    async fn get_validators(url: String) -> Result<SuiSystemStateSummary> {
        let rpc_method = "suix_getLatestSuiSystemState";
        let observe = || {
            let timer = JSON_RPC_DURATION
                .with_label_values(&[rpc_method])
                .start_timer();
            || {
                timer.observe_duration();
            }
        }();
        let client = reqwest::Client::builder().build().unwrap();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method":rpc_method,
            "id":1,
        });
        let response = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.to_string())
            .send()
            .await
            .with_context(|| {
                JSON_RPC_STATE
                    .with_label_values(&[rpc_method, "failed_get"])
                    .inc();
                observe();
                "unable to perform json rpc"
            })?;

        let raw = response.bytes().await.with_context(|| {
            JSON_RPC_STATE
                .with_label_values(&[rpc_method, "failed_body_extract"])
                .inc();
            observe();
            "unable to extract body bytes from json rpc"
        })?;

        #[derive(Debug, Deserialize)]
        struct ResponseBody {
            result: SuiSystemStateSummary,
        }

        let body: ResponseBody = match serde_json::from_slice(&raw) {
            Ok(b) => b,
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&[rpc_method, "failed_json_decode"])
                    .inc();
                observe();
                bail!(
                    "unable to decode json: {error} response from json rpc: {:?}",
                    raw
                )
            }
        };
        JSON_RPC_STATE
            .with_label_values(&[rpc_method, "success"])
            .inc();
        observe();
        Ok(body.result)
    }

    /// poll_peer_list will act as a refresh interval for our cache
    pub fn poll_peer_list(&self) {
        info!("Started polling for peers using rpc: {}", self.rpc_url);

        let rpc_poll_interval = self.rpc_poll_interval;
        let rpc_url = self.rpc_url.to_owned();
        let bridge_rpc_url = self.rpc_url.to_owned();
        let bridge_nodes = self.bridge_nodes.clone();
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
                        info!(
                            "{} validator peers managed to make it on the allow list",
                            allow.len()
                        );
                        JSON_RPC_STATE
                            .with_label_values(&["update_peer_count", "success"])
                            .inc_by(allow.len() as f64);
                    }
                    Err(error) => {
                        JSON_RPC_STATE
                            .with_label_values(&["update_peer_count", "failed"])
                            .inc();
                        error!("unable to refresh peer list: {error}")
                    }
                }
            }
        });
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(rpc_poll_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                match Self::get_bridge_validators(bridge_rpc_url.clone()).await {
                    Ok(summary) => {
                        let extracted = extract_bridge(summary).await;
                        let mut allow = bridge_nodes.write().unwrap();
                        allow.clear();
                        allow.extend(extracted);
                        info!(
                            "{} sui bridge peers managed to make it on the allow list",
                            allow.len()
                        );
                        JSON_RPC_STATE
                            .with_label_values(&["update_bridge_peer_count", "success"])
                            .inc_by(allow.len() as f64);
                    }
                    Err(error) => {
                        JSON_RPC_STATE
                            .with_label_values(&["update_bridge_peer_count", "failed"])
                            .inc();
                        error!("unable to refresh sui bridge peer list: {error}")
                    }
                }
            }
        });
    }
}

async fn extract_bridge(summary: BridgeSummary) -> Vec<(Ed25519PublicKey, BridgePeer)> {
    let client = reqwest::Client::builder().build().unwrap();
    let results: Vec<_> = stream::iter(summary.committee.members)
        .filter_map(|(_, cm)| {
            let client = client.clone();
            async move {
                // TODO: handle unwrap maybe url validate
                let bridge_node_url =
                    String::from_utf8(cm.http_rest_url).ok()? + "/metrics_pub_key";

                let response = client.get(&bridge_node_url).send().await.ok()?;
                let raw = response.bytes().await.ok()?;
                // Try to deserialize the raw bytes into a string
                let metrics_pub_key: String = match serde_json::from_slice(&raw) {
                    Ok(key) => key,
                    Err(error) => {
                        error!("Failed to deserialize response: {:?}", error);
                        return None;
                    }
                };
                // info!("here's the whole key: {:?}", metrics_pub_key);
                // match Ed25519KeyPair::decode_base64(&metrics_pub_key) {
                //     Ok(key) => {
                //         let pubickey: Ed25519PublicKey = key.public().clone();
                //         error!("here's the pub key: {:?}", pubickey);
                //     }
                //     Err(error) => {
                //         error!(
                //             "unable to decode key pair for bridge node {:?} error: {error}",
                //             bridge_node_url
                //         );
                //     }
                // }
                match Ed25519KeyPair::decode_base64(&metrics_pub_key) {
                    Ok(metrics_key) => {
                        debug!(
                            "adding metrics key {:?} for sui address {:?}",
                            metrics_key, bridge_node_url
                        );
                        Some((
                            metrics_key.public().clone(),
                            BridgePeer {
                                sui_address: cm.sui_address.clone(),
                                public_key: metrics_key.public().clone(),
                            },
                        ))
                    }
                    Err(error) => {
                        error!(
                            "unable to decode public key for bridge node {:?} error: {error}",
                            bridge_node_url
                        );
                        None
                    }
                }
            }
        })
        .collect()
        .await;

    results
}

// async fn extract_bridge(
//     summary: BridgeSummary,
// ) -> impl Iterator<Item = (Ed25519PublicKey, BridgePeer)> {
//     let client = reqwest::Client::builder().build().unwrap();
// summary.committee.members.into_iter().filter_map(|(_, cm)| {

//     summary.committee.members.into_iter().filter_map(|(_, cm)| {
//         // TODO: handle unwrap maybe url validate
//         let bridge_node_url = String::from_utf8(cm.http_rest_url).unwrap() + "/metrics_pub_key";

//         let response = client.get(bridge_node_url).send().await?;

//         let raw = response.bytes().await?;

//         #[derive(Debug, Deserialize)]
//         struct ResponseBody {
//             metrics_pub_key: String,
//         }

//         let body: ResponseBody = match serde_json::from_slice(&raw) {
//             Ok(b) => b,
//             Err(error) => {
//                 error!("shit");
//                 todo!();
//             }
//         };

//         match Ed25519PublicKey::from_bytes(&raw.to_vec()) {
//             Ok(public_key) => {
//                 debug!(
//                     "adding public key {:?} for sui address {:?}",
//                     public_key, cm.sui_address
//                 );
//                 Some((
//                     public_key.clone(),
//                     BridgePeer {
//                         sui_address: cm.sui_address,
//                         public_key,
//                     },
//                 )) // scoped to filter_map
//             }
//             Err(error) => {
//                 error!(
//                     "unable to decode public key for bridge node sui_address: {:?} error: {error}",
//                     cm.sui_address
//                 );
//                 None // scoped to filter_map
//             }
//         }
//     })
// }

/// extract will get the network pubkey bytes from a SuiValidatorSummary type.  This type comes from a
/// full node rpc result.  See get_validators for details.  The key here, if extracted successfully, will
/// ultimately be stored in the allow list and let us communicate with those actual peers via tls.
fn extract(summary: SuiSystemStateSummary) -> impl Iterator<Item = (Ed25519PublicKey, SuiPeer)> {
    summary.active_validators.into_iter().filter_map(|vm| {
        match Ed25519PublicKey::from_bytes(&vm.network_pubkey_bytes) {
            Ok(public_key) => {
                let Ok(p2p_address) = Multiaddr::try_from(vm.p2p_address) else {
                    error!(
                        "refusing to add peer to allow list; unable to decode multiaddr for {}",
                        vm.name
                    );
                    return None; // scoped to filter_map
                };
                debug!(
                    "adding public key {:?} for address {:?}",
                    public_key, p2p_address
                );
                Some((
                    public_key.clone(),
                    SuiPeer {
                        name: vm.name,
                        p2p_address,
                        public_key,
                    },
                )) // scoped to filter_map
            }
            Err(error) => {
                error!(
                    "unable to decode public key for name: {:?} sui_address: {:?} error: {error}",
                    vm.name, vm.sui_address
                );
                None // scoped to filter_map
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
        base_types::ObjectID,
        bridge::{
            BridgeCommitteeSummary, BridgeLimiterSummary, BridgeTreasurySummary,
            MoveTypeBridgeCommittee, MoveTypeCommitteeMember,
        },
        sui_system_state::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
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
            active_validators: vec![SuiValidatorSummary {
                network_pubkey_bytes: Vec::from(client_pub_key.as_bytes()),
                p2p_address: format!("{p2p_address}"),
                primary_address: "empty".into(),
                worker_address: "empty".into(),
                ..Default::default()
            }],
            ..Default::default()
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

    #[test]
    fn extract_bridge_summary() {
        let CertKeyPair(_, client_pub_key) = generate_self_cert("sui".into());
        let bridge_summary = BridgeSummary {
            committee: BridgeCommitteeSummary {
                members: vec![
                    (
                        vec![1],
                        MoveTypeCommitteeMember {
                            sui_address: SuiAddress::ZERO,
                            bridge_pubkey_bytes: Vec::from(client_pub_key.as_bytes()),
                            ..Default::default()
                        },
                    ),
                    (
                        vec![2],
                        MoveTypeCommitteeMember {
                            sui_address: SuiAddress::random_for_testing_only(),
                            bridge_pubkey_bytes: Vec::from(client_pub_key.as_bytes()),
                            ..Default::default()
                        },
                    ),
                ],
                ..Default::default()
            },
            bridge_version: 1,
            message_version: 1,
            chain_id: 1,
            sequence_nums: vec![(1, 2)],
            treasury: BridgeTreasurySummary {
                ..Default::default()
            },
            bridge_records_id: ObjectID::random(),
            limiter: BridgeLimiterSummary {
                ..Default::default()
            },
            is_frozen: false,
        };

        let bridge_peers: Vec<_> = extract_bridge(bridge_summary).collect();

        assert_eq!(
            bridge_peers.len(),
            2,
            "peers should have been a length of 2"
        );
        assert_eq!(
            bridge_peers[0].1.sui_address,
            SuiAddress::ZERO,
            "sui address should have been 0x0"
        )
    }
}
