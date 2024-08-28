// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{bail, Context, Result};
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use fastcrypto::traits::ToFromBytes;
use futures::stream::{self, StreamExt};
use once_cell::sync::Lazy;
use prometheus::{register_counter_vec, register_histogram_vec};
use prometheus::{CounterVec, HistogramVec};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_tls::Allower;
use sui_types::bridge::BridgeSummary;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use tracing::{debug, error, info};
use url::Url;

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

/// AllowedPeers is a mapping of public key to AllowedPeer data
pub type AllowedPeers = Arc<RwLock<HashMap<Ed25519PublicKey, AllowedPeer>>>;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct AllowedPeer {
    pub name: String,
    pub public_key: Ed25519PublicKey,
}

/// SuiNodeProvider queries the sui blockchain and keeps a record of known validators based on the response from
/// sui_getValidators.  The node name, public key and other info is extracted from the chain and stored in this
/// data structure.  We pass this struct to the tls verifier and it depends on the state contained within.
/// Handlers also use this data in an Extractor extension to check incoming clients on the http api against known keys.
#[derive(Debug, Clone)]
pub struct SuiNodeProvider {
    nodes: AllowedPeers,
    static_nodes: AllowedPeers,
    rpc_url: String,
    rpc_poll_interval: Duration,
}

impl Allower for SuiNodeProvider {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        self.static_nodes.read().unwrap().contains_key(key)
            || self.nodes.read().unwrap().contains_key(key)
    }
}

impl SuiNodeProvider {
    pub fn new(
        rpc_url: String,
        rpc_poll_interval: Duration,
        static_peers: Vec<AllowedPeer>,
    ) -> Self {
        // build our hashmap with the static pub keys. we only do this one time at binary startup.
        let static_nodes: HashMap<Ed25519PublicKey, AllowedPeer> = static_peers
            .into_iter()
            .map(|v| (v.public_key.clone(), v))
            .collect();
        let static_nodes = Arc::new(RwLock::new(static_nodes));
        let nodes = Arc::new(RwLock::new(HashMap::new()));
        Self {
            nodes,
            static_nodes,
            rpc_url,
            rpc_poll_interval,
        }
    }

    /// get is used to retrieve peer info in our handlers
    pub fn get(&self, key: &Ed25519PublicKey) -> Option<AllowedPeer> {
        debug!("look for {:?}", key);
        // check static nodes first
        if let Some(v) = self.static_nodes.read().unwrap().get(key) {
            return Some(AllowedPeer {
                name: v.name.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        // check dynamic nodes
        if let Some(v) = self.nodes.read().unwrap().get(key) {
            return Some(AllowedPeer {
                name: v.name.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        None
    }
    /// Get a reference to the inner service
    pub fn get_ref(&self) -> &AllowedPeers {
        &self.nodes
    }

    /// Get a mutable reference to the inner service
    pub fn get_mut(&mut self) -> &mut AllowedPeers {
        &mut self.nodes
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

    /// get_bridge_validators will retrieve known bridge validators
    async fn get_bridge_validators(url: String) -> Result<BridgeSummary> {
        let rpc_method = "suix_getLatestBridge";
        let _timer = JSON_RPC_DURATION
            .with_label_values(&[rpc_method])
            .start_timer();
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
                "unable to perform json rpc"
            })?;

        let raw = response.bytes().await.with_context(|| {
            JSON_RPC_STATE
                .with_label_values(&[rpc_method, "failed_body_extract"])
                .inc();
            "unable to extract body bytes from json rpc"
        })?;

        #[derive(Debug, Deserialize)]
        struct ResponseBody {
            result: BridgeSummary,
        }
        let summary: BridgeSummary = match serde_json::from_slice::<ResponseBody>(&raw) {
            Ok(b) => b.result,
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&[rpc_method, "failed_json_decode"])
                    .inc();
                bail!(
                    "unable to decode json: {error} response from json rpc: {:?}",
                    raw
                )
            }
        };
        JSON_RPC_STATE
            .with_label_values(&[rpc_method, "success"])
            .inc();
        Ok(summary)
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
                        info!(
                            "{} sui peers managed to make it on the allow list",
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
                match Self::get_bridge_validators(rpc_url.to_owned()).await {
                    Ok(summary) => {
                        let extracted = extract_bridge(summary).await;
                        let mut allow = nodes.write().unwrap();
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

/// extract will get the network pubkey bytes from a SuiValidatorSummary type.  This type comes from a
/// full node rpc result.  See get_validators for details.  The key here, if extracted successfully, will
/// ultimately be stored in the allow list and let us communicate with those actual peers via tls.
fn extract(
    summary: SuiSystemStateSummary,
) -> impl Iterator<Item = (Ed25519PublicKey, AllowedPeer)> {
    summary.active_validators.into_iter().filter_map(|vm| {
        match Ed25519PublicKey::from_bytes(&vm.network_pubkey_bytes) {
            Ok(public_key) => {
                debug!(
                    "adding public key {:?} for sui validator {:?}",
                    public_key, vm.name
                );
                Some((
                    public_key.clone(),
                    AllowedPeer {
                        name: vm.name,
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

async fn extract_bridge(summary: BridgeSummary) -> Vec<(Ed25519PublicKey, AllowedPeer)> {
    let client = reqwest::Client::builder().build().unwrap();
    let results: Vec<_> = stream::iter(summary.committee.members)
        .filter_map(|(_, cm)| {
            let client = client.clone();
            async move {
                debug!(
                    address =% cm.sui_address,
                    "Extracting metrics public key for bridge node",
                );

                // Convert the Vec<u8> to a String and handle errors properly
                let url_str = match String::from_utf8(cm.http_rest_url) {
                    Ok(url) => url,
                    Err(_) => {
                        error!(
                            address =% cm.sui_address,
                            "Invalid UTF-8 sequence in http_rest_url for bridge node ",
                        );
                        return None;
                    }
                };
                // Parse the URL
                let mut bridge_url = match Url::parse(&url_str) {
                    Ok(url) => url,
                    Err(_) => {
                        error!(url_str, "Unable to parse http_rest_url");
                        return None;
                    }
                };
                bridge_url.set_path("/metrics_pub_key");

                // use the host portion of the http_rest_url as the "name"
                let bridge_host = match bridge_url.host_str() {
                    Some(host) => host,
                    None => {
                        error!(url_str, "Hostname is missing from http_rest_url");
                        return None;
                    }
                };
                let bridge_name = String::from(bridge_host);
                let bridge_request_url = bridge_url.as_str();

                let response = client.get(bridge_request_url).send().await.ok()?;
                let raw = response.bytes().await.ok()?;
                // Try to deserialize the raw bytes into a string
                let metrics_pub_key: String = match serde_json::from_slice(&raw) {
                    Ok(key) => key,
                    Err(error) => {
                        error!(?error, "Failed to deserialize response");
                        return None;
                    }
                };
                let metrics_bytes = match Base64::decode(&metrics_pub_key) {
                    Ok(pubkey_bytes) => pubkey_bytes,
                    Err(error) => {
                        error!(
                            ?error,
                            bridge_name, "unable to decode public key for bridge node",
                        );
                        return None;
                    }
                };
                match Ed25519PublicKey::from_bytes(&metrics_bytes) {
                    Ok(metrics_key) => {
                        debug!(bridge_request_url, ?metrics_key, "adding metrics key");
                        Some((
                            metrics_key.clone(),
                            AllowedPeer {
                                public_key: metrics_key.clone(),
                                name: bridge_name,
                            },
                        ))
                    }
                    Err(error) => {
                        error!(
                            ?error,
                            bridge_request_url, "unable to decode public key for bridge node",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{generate_self_cert, CertKeyPair};
    use serde::Serialize;
    use sui_types::sui_system_state::sui_system_state_summary::{
        SuiSystemStateSummary, SuiValidatorSummary,
    };

    /// creates a test that binds our proxy use case to the structure in sui_getLatestSuiSystemState
    /// most of the fields are garbage, but we will send the results of the serde process to a private decode
    /// function that should always work if the structure is valid for our use
    #[test]
    fn depend_on_sui_sui_system_state_summary() {
        let CertKeyPair(_, client_pub_key) = generate_self_cert("sui".into());
        // all fields here just satisfy the field types, with exception to active_validators, we use
        // some of those.
        let depends_on = SuiSystemStateSummary {
            active_validators: vec![SuiValidatorSummary {
                network_pubkey_bytes: Vec::from(client_pub_key.as_bytes()),
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
}
