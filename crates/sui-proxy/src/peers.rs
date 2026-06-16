// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result, bail};
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use fastcrypto::traits::ToFromBytes;
use futures::stream::{self, StreamExt};
use once_cell::sync::Lazy;
use prometheus::{CounterVec, HistogramVec, IntGaugeVec};
use prometheus::{register_counter_vec, register_histogram_vec, register_int_gauge_vec};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_tls::Allower;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeSummary;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use tracing::{debug, error, info, warn};
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

/// The on-chain hashi committee epoch as last observed by the resolver. A flatlining
/// value relative to the actual chain epoch indicates the resolver is stuck.
static HASHI_OBSERVED_EPOCH: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "hashi_proxy_observed_committee_epoch",
        "Most recent hashi CommitteeSet.epoch observed by the resolver.",
        &["hashi_object_id"]
    )
    .unwrap()
});

/// Number of hashi members currently in the allowlist (current + pending committee with
/// a valid 32-byte tls_public_key).
static HASHI_ALLOWED_MEMBERS: Lazy<IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "hashi_proxy_allowed_members",
        "Number of hashi members on the proxy allowlist.",
        &["hashi_object_id"]
    )
    .unwrap()
});

/// AllowedPeers is a mapping of public key to AllowedPeer data
pub type AllowedPeers = Arc<RwLock<HashMap<Ed25519PublicKey, AllowedPeer>>>;

type MetricsPubKeys = Arc<RwLock<HashMap<String, Ed25519PublicKey>>>;

#[derive(Hash, PartialEq, Eq, Debug, Clone)]
pub struct AllowedPeer {
    pub name: String,
    pub public_key: Ed25519PublicKey,
}

/// Cache of `SuiAddress -> validator name` from the latest sui system state poll.
/// Used by bridge/hashi resolvers to label peers by friendly validator name without
/// each resolver re-fetching the validator set.
type ValidatorNames = Arc<RwLock<BTreeMap<SuiAddress, String>>>;

/// SuiNodeProvider queries the sui blockchain and keeps a record of known validators based on the response from
/// sui_getValidators.  The node name, public key and other info is extracted from the chain and stored in this
/// data structure.  We pass this struct to the tls verifier and it depends on the state contained within.
/// Handlers also use this data in an Extractor extension to check incoming clients on the http api against known keys.
#[derive(Debug, Clone)]
pub struct SuiNodeProvider {
    sui_nodes: AllowedPeers,
    bridge_nodes: AllowedPeers,
    hashi_nodes: AllowedPeers,
    static_nodes: AllowedPeers,
    sui_validator_names: ValidatorNames,
    rpc_url: String,
    rpc_poll_interval: Duration,
    /// Object ID of the `hashi::hashi::Hashi` shared object on the chain identified
    /// by `rpc_url`. `None` disables the hashi resolver entirely.
    hashi_object_id: Option<String>,
}

impl Allower for SuiNodeProvider {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        self.static_nodes.read().unwrap().contains_key(key)
            || self.sui_nodes.read().unwrap().contains_key(key)
            || self.bridge_nodes.read().unwrap().contains_key(key)
            || self.hashi_nodes.read().unwrap().contains_key(key)
    }
}

impl SuiNodeProvider {
    pub fn new(
        rpc_url: String,
        rpc_poll_interval: Duration,
        static_peers: Vec<AllowedPeer>,
        hashi_object_id: Option<String>,
    ) -> Self {
        // build our hashmap with the static pub keys. we only do this one time at binary startup.
        let static_nodes: HashMap<Ed25519PublicKey, AllowedPeer> = static_peers
            .into_iter()
            .map(|v| (v.public_key.clone(), v))
            .collect();
        let static_nodes = Arc::new(RwLock::new(static_nodes));
        let sui_nodes = Arc::new(RwLock::new(HashMap::new()));
        let bridge_nodes = Arc::new(RwLock::new(HashMap::new()));
        let hashi_nodes = Arc::new(RwLock::new(HashMap::new()));
        let sui_validator_names = Arc::new(RwLock::new(BTreeMap::new()));
        Self {
            sui_nodes,
            bridge_nodes,
            hashi_nodes,
            static_nodes,
            sui_validator_names,
            rpc_url,
            rpc_poll_interval,
            hashi_object_id,
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
        // check sui validators
        if let Some(v) = self.sui_nodes.read().unwrap().get(key) {
            return Some(AllowedPeer {
                name: v.name.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        // check bridge validators
        if let Some(v) = self.bridge_nodes.read().unwrap().get(key) {
            return Some(AllowedPeer {
                name: v.name.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        // check hashi committee members
        if let Some(v) = self.hashi_nodes.read().unwrap().get(key) {
            return Some(AllowedPeer {
                name: v.name.to_owned(),
                public_key: v.public_key.to_owned(),
            });
        }
        None
    }

    /// Get a mutable reference to the allowed sui validator map
    pub fn get_sui_mut(&mut self) -> &mut AllowedPeers {
        &mut self.sui_nodes
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

    async fn update_sui_validator_set(&self) {
        match Self::get_validators(self.rpc_url.to_owned()).await {
            Ok(summary) => {
                // Snapshot the validator-address -> name map for downstream resolvers
                // (bridge/hashi) before we hand `summary.active_validators` off to the
                // network-key extractor.
                let names: BTreeMap<SuiAddress, String> = summary
                    .active_validators
                    .iter()
                    .map(|v| (v.sui_address, v.name.clone()))
                    .collect();
                {
                    let mut nw = self.sui_validator_names.write().unwrap();
                    *nw = names;
                }

                let validators = extract(summary);
                let mut allow = self.sui_nodes.write().unwrap();
                allow.clear();
                allow.extend(validators);
                info!(
                    "{} sui validators managed to make it on the allow list",
                    allow.len()
                );
            }
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&["update_peer_count", "failed"])
                    .inc();
                error!("unable to refresh peer list: {error}");
            }
        };
    }

    async fn update_hashi_committee_set(&self, hashi_object_id: &str) {
        let validator_names: BTreeMap<SuiAddress, String> =
            self.sui_validator_names.read().unwrap().clone();

        match resolve_hashi_committee(&self.rpc_url, hashi_object_id, &validator_names).await {
            Ok(result) => {
                HASHI_OBSERVED_EPOCH
                    .with_label_values(&[hashi_object_id])
                    .set(result.epoch as i64);
                HASHI_ALLOWED_MEMBERS
                    .with_label_values(&[hashi_object_id])
                    .set(result.peers.len() as i64);
                let mut allow = self.hashi_nodes.write().unwrap();
                allow.clear();
                allow.extend(result.peers);
                info!(
                    epoch = result.epoch,
                    pending_epoch = ?result.pending_epoch,
                    "{} hashi members on the allow list",
                    allow.len(),
                );
            }
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&["update_hashi_committee_set", "failed"])
                    .inc();
                error!("unable to refresh hashi peer list: {error:#}");
            }
        }
    }

    async fn update_bridge_validator_set(&self, metrics_keys: MetricsPubKeys) {
        let sui_system = match Self::get_validators(self.rpc_url.to_owned()).await {
            Ok(summary) => summary,
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&["update_bridge_peer_count", "failed"])
                    .inc();
                error!("unable to get sui system state: {error}");
                return;
            }
        };
        match Self::get_bridge_validators(self.rpc_url.to_owned()).await {
            Ok(summary) => {
                let names = sui_system
                    .active_validators
                    .into_iter()
                    .map(|v| (v.sui_address, v.name))
                    .collect();
                let validators = extract_bridge(summary, Arc::new(names), metrics_keys).await;
                let mut allow = self.bridge_nodes.write().unwrap();
                allow.clear();
                allow.extend(validators);
                info!(
                    "{} bridge validators managed to make it on the allow list",
                    allow.len()
                );
            }
            Err(error) => {
                JSON_RPC_STATE
                    .with_label_values(&["update_bridge_peer_count", "failed"])
                    .inc();
                error!("unable to refresh sui bridge peer list: {error}");
            }
        };
    }

    /// poll_peer_list will act as a refresh interval for our cache
    pub fn poll_peer_list(&self) {
        info!("Started polling for peers using rpc: {}", self.rpc_url);

        let rpc_poll_interval = self.rpc_poll_interval;
        let cloned_self = self.clone();
        let bridge_metrics_keys: MetricsPubKeys = Arc::new(RwLock::new(HashMap::new()));
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(rpc_poll_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                // The sui validator set must update first; bridge and hashi resolvers
                // read the cached `sui_validator_names` for friendly labeling.
                cloned_self.update_sui_validator_set().await;
                cloned_self
                    .update_bridge_validator_set(bridge_metrics_keys.clone())
                    .await;
                if let Some(hashi_object_id) = cloned_self.hashi_object_id.as_deref() {
                    cloned_self
                        .update_hashi_committee_set(hashi_object_id)
                        .await;
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

async fn extract_bridge(
    summary: BridgeSummary,
    names: Arc<BTreeMap<SuiAddress, String>>,
    metrics_keys: MetricsPubKeys,
) -> Vec<(Ed25519PublicKey, AllowedPeer)> {
    {
        // Clean up the cache: retain only the metrics keys of the up-to-date bridge validator set
        let mut metrics_keys_write = metrics_keys.write().unwrap();
        metrics_keys_write.retain(|url, _| {
            summary.committee.members.iter().any(|(_, cm)| {
                String::from_utf8(cm.http_rest_url.clone()).ok().as_ref() == Some(url)
            })
        });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let committee_members = summary.committee.members.clone();
    let results: Vec<_> = stream::iter(committee_members)
        .filter_map(|(_, cm)| {
            let client = client.clone();
            let metrics_keys = metrics_keys.clone();
            let names = names.clone();
            async move {
                debug!(
                    address =% cm.sui_address,
                    "Extracting metrics public key for bridge node",
                );

                // Convert the Vec<u8> to a String and handle errors properly
                let url_str = match String::from_utf8(cm.http_rest_url) {
                    Ok(url) => url,
                    Err(_) => {
                        warn!(
                            address =% cm.sui_address,
                            "Invalid UTF-8 sequence in http_rest_url for bridge node ",
                        );
                        return None;
                    }
                };
                // Parse the URL
                let bridge_url = match Url::parse(&url_str) {
                    Ok(url) => url,
                    Err(_) => {
                        warn!(url_str, "Unable to parse http_rest_url");
                        return None;
                    }
                };

                // Append "metrics_pub_key" to the path
                let bridge_url = match append_path_segment(bridge_url, "metrics_pub_key") {
                    Some(url) => url,
                    None => {
                        warn!(url_str, "Unable to append path segment to URL");
                        return None;
                    }
                };

                // Use the host portion of the http_rest_url as the "name"
                let bridge_host = match bridge_url.host_str() {
                    Some(host) => host,
                    None => {
                        warn!(url_str, "Hostname is missing from http_rest_url");
                        return None;
                    }
                };
                let bridge_name = names.get(&cm.sui_address).cloned().unwrap_or_else(|| {
                    warn!(
                        address =% cm.sui_address,
                        "Bridge node not found in sui committee, using base URL as the name",
                    );
                    String::from(bridge_host)
                });
                let bridge_name = format!("bridge-{}", bridge_name);

                let bridge_request_url = bridge_url.as_str();

                let metrics_pub_key = match client.get(bridge_request_url).send().await {
                    Ok(response) => {
                        let raw = response.bytes().await.ok()?;
                        let metrics_pub_key: String = match serde_json::from_slice(&raw) {
                            Ok(key) => key,
                            Err(error) => {
                                warn!(?error, url_str, "Failed to deserialize response");
                                return fallback_to_cached_key(
                                    &metrics_keys,
                                    &url_str,
                                    &bridge_name,
                                );
                            }
                        };
                        let metrics_bytes = match Base64::decode(&metrics_pub_key) {
                            Ok(pubkey_bytes) => pubkey_bytes,
                            Err(error) => {
                                warn!(
                                    ?error,
                                    bridge_name, "unable to decode public key for bridge node",
                                );
                                return None;
                            }
                        };
                        match Ed25519PublicKey::from_bytes(&metrics_bytes) {
                            Ok(pubkey) => {
                                // Successfully fetched the key, update the cache
                                let mut metrics_keys_write = metrics_keys.write().unwrap();
                                metrics_keys_write.insert(url_str.clone(), pubkey.clone());
                                debug!(
                                    url_str,
                                    public_key = ?pubkey,
                                    "Successfully added bridge peer to metrics_keys"
                                );
                                pubkey
                            }
                            Err(error) => {
                                warn!(
                                    ?error,
                                    bridge_request_url,
                                    "unable to decode public key for bridge node",
                                );
                                return None;
                            }
                        }
                    }
                    Err(_) => {
                        return fallback_to_cached_key(&metrics_keys, &url_str, &bridge_name);
                    }
                };
                Some((
                    metrics_pub_key.clone(),
                    AllowedPeer {
                        public_key: metrics_pub_key,
                        name: bridge_name,
                    },
                ))
            }
        })
        .collect()
        .await;

    results
}

fn fallback_to_cached_key(
    metrics_keys: &MetricsPubKeys,
    url_str: &str,
    bridge_name: &str,
) -> Option<(Ed25519PublicKey, AllowedPeer)> {
    let metrics_keys_read = metrics_keys.read().unwrap();
    if let Some(cached_key) = metrics_keys_read.get(url_str) {
        debug!(
            url_str,
            "Using cached metrics public key after request failure"
        );
        Some((
            cached_key.clone(),
            AllowedPeer {
                public_key: cached_key.clone(),
                name: bridge_name.to_string(),
            },
        ))
    } else {
        warn!(
            url_str,
            "Failed to fetch public key and no cached key available"
        );
        None
    }
}

fn append_path_segment(mut url: Url, segment: &str) -> Option<Url> {
    url.path_segments_mut().ok()?.pop_if_empty().push(segment);
    Some(url)
}

// On-chain shape we depend on (see hashi/packages/hashi/sources/core/committee/
// committee_set.move and hashi.move):
//
//   Hashi (shared object, key)
//     committee_set: CommitteeSet (store)
//       epoch: u64
//       pending_epoch_change: Option<u64>
//       members: Bag<address, MemberInfo>
//       committees: Bag<u64, Committee>
//
//   MemberInfo
//     validator_address: address
//     tls_public_key: vector<u8>         // 32-byte raw ed25519; empty until set
//
//   Committee.members: vector<CommitteeMember>
//   CommitteeMember.validator_address: address

/// Snapshot of the CommitteeSet metadata pulled from one `sui_getObject` call.
#[derive(Debug, Clone)]
struct CommitteeSetSnapshot {
    epoch: u64,
    pending_epoch: Option<u64>,
    members_bag_id: String,
    committees_bag_id: String,
}

/// A single hashi member resolved from the on-chain `members` Bag. The
/// `tls_public_key` may be empty for members that registered but haven't yet
/// called `set_tls_public_key` — callers filter those out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHashiMember {
    pub validator_address: SuiAddress,
    pub tls_public_key: Vec<u8>,
}

/// Output of `resolve_hashi_committee`: allowlist contents plus epoch info for
/// observability metrics.
#[derive(Debug)]
struct HashiResolution {
    epoch: u64,
    pending_epoch: Option<u64>,
    peers: Vec<(Ed25519PublicKey, AllowedPeer)>,
}

/// End-to-end resolve: RPC -> peer allowlist entries.
async fn resolve_hashi_committee(
    rpc_url: &str,
    hashi_object_id: &str,
    validator_names: &BTreeMap<SuiAddress, String>,
) -> Result<HashiResolution> {
    let snapshot = get_hashi_committee_snapshot(rpc_url, hashi_object_id).await?;
    debug!(
        epoch = snapshot.epoch,
        pending_epoch = ?snapshot.pending_epoch,
        "fetched hashi committee snapshot"
    );

    // Union of validator_addresses across active and pending committees; the
    // set tolerates the expected overlap during reconfig. A missing current
    // Committee is not an error — at genesis the `committees` Bag is empty
    // until the first `start_reconfig` runs.
    let mut active_addrs: std::collections::HashSet<SuiAddress> =
        match get_committee_validator_addresses(
            rpc_url,
            &snapshot.committees_bag_id,
            snapshot.epoch,
        )
        .await
        {
            Ok(addrs) => addrs.into_iter().collect(),
            Err(e) => {
                debug!(
                    epoch = snapshot.epoch,
                    "no Committee at current epoch (pre-genesis or between reconfigs?): {e:#}"
                );
                std::collections::HashSet::new()
            }
        };
    if let Some(next) = snapshot.pending_epoch {
        match get_committee_validator_addresses(rpc_url, &snapshot.committees_bag_id, next).await {
            Ok(addrs) => active_addrs.extend(addrs),
            Err(e) => warn!(
                pending_epoch = next,
                "could not fetch pending committee: {e:#}",
            ),
        }
    }

    // Fetch each member's MemberInfo concurrently. 100ish validators per chain,
    // bounded concurrency keeps RPC load reasonable without serializing.
    let members: Vec<ResolvedHashiMember> = stream::iter(active_addrs)
        .map(|addr| {
            let url = rpc_url.to_owned();
            let bag = snapshot.members_bag_id.clone();
            async move {
                match get_hashi_member_info(&url, &bag, addr).await {
                    Ok(m) => Some(m),
                    Err(e) => {
                        warn!(addr =% addr, "could not fetch hashi MemberInfo: {e:#}");
                        None
                    }
                }
            }
        })
        .buffer_unordered(16)
        .filter_map(|x| async move { x })
        .collect()
        .await;

    let peers = extract_hashi(members, validator_names);

    Ok(HashiResolution {
        epoch: snapshot.epoch,
        pending_epoch: snapshot.pending_epoch,
        peers,
    })
}

/// Filter to members with a valid 32-byte tls_public_key and build AllowedPeer
/// entries labeled `hashi-<validator name>`.
fn extract_hashi(
    members: Vec<ResolvedHashiMember>,
    validator_names: &BTreeMap<SuiAddress, String>,
) -> Vec<(Ed25519PublicKey, AllowedPeer)> {
    members
        .into_iter()
        .filter_map(|m| {
            if m.tls_public_key.len() != 32 {
                debug!(
                    addr =% m.validator_address,
                    "skipping hashi member with empty/invalid tls_public_key"
                );
                return None;
            }
            let pk = match Ed25519PublicKey::from_bytes(&m.tls_public_key) {
                Ok(pk) => pk,
                Err(error) => {
                    warn!(
                        addr =% m.validator_address,
                        ?error,
                        "invalid tls_public_key bytes for hashi member",
                    );
                    return None;
                }
            };
            let name = validator_names
                .get(&m.validator_address)
                .cloned()
                .unwrap_or_else(|| m.validator_address.to_string());
            let labelled = format!("hashi-{name}");
            debug!(
                addr =% m.validator_address,
                public_key = ?pk,
                "adding hashi member to allow list as {labelled}",
            );
            Some((
                pk.clone(),
                AllowedPeer {
                    name: labelled,
                    public_key: pk,
                },
            ))
        })
        .collect()
}

async fn get_hashi_committee_snapshot(
    rpc_url: &str,
    hashi_object_id: &str,
) -> Result<CommitteeSetSnapshot> {
    let rpc_method = "sui_getObject";
    let _timer = JSON_RPC_DURATION
        .with_label_values(&[rpc_method])
        .start_timer();

    let body = sui_rpc_call(
        rpc_url,
        rpc_method,
        serde_json::json!([
            hashi_object_id,
            {"showContent": true}
        ]),
    )
    .await?;

    let fields = body
        .pointer("/result/data/content/fields/committee_set/fields")
        .context("missing committee_set fields in Hashi object response")?;

    let epoch = parse_u64_field(fields.get("epoch"), "committee_set.epoch")?;
    let pending_epoch = match fields.get("pending_epoch_change") {
        // Move's `Option<u64>` serializes as either null or {"vec": ["<n>"]}
        // depending on display path; tolerate both.
        Some(Value::Null) | None => None,
        Some(other) => parse_optional_u64(other)?,
    };
    let members_bag_id = parse_bag_id(fields.get("members"), "committee_set.members")?;
    let committees_bag_id = parse_bag_id(fields.get("committees"), "committee_set.committees")?;

    Ok(CommitteeSetSnapshot {
        epoch,
        pending_epoch,
        members_bag_id,
        committees_bag_id,
    })
}

async fn get_committee_validator_addresses(
    rpc_url: &str,
    committees_bag_id: &str,
    epoch: u64,
) -> Result<Vec<SuiAddress>> {
    let rpc_method = "suix_getDynamicFieldObject";
    let _timer = JSON_RPC_DURATION
        .with_label_values(&[rpc_method])
        .start_timer();

    let body = sui_rpc_call(
        rpc_url,
        rpc_method,
        serde_json::json!([
            committees_bag_id,
            {"type": "u64", "value": epoch.to_string()}
        ]),
    )
    .await?;

    // Dynamic-field objects wrap their value as a Move Field<K, V>; the value
    // field name is "value" and its contents are the Committee fields.
    let committee_fields = body
        .pointer("/result/data/content/fields/value/fields")
        .with_context(|| {
            format!("missing Committee fields for epoch {epoch} under {committees_bag_id}")
        })?;

    let members = committee_fields
        .get("members")
        .and_then(Value::as_array)
        .with_context(|| format!("Committee.members is not an array for epoch {epoch}"))?;

    let mut out = Vec::with_capacity(members.len());
    for m in members {
        let addr_str = m
            .pointer("/fields/validator_address")
            .and_then(Value::as_str)
            .context("missing CommitteeMember.validator_address")?;
        let addr: SuiAddress = addr_str
            .parse()
            .with_context(|| format!("parsing CommitteeMember.validator_address: {addr_str}"))?;
        out.push(addr);
    }
    Ok(out)
}

async fn get_hashi_member_info(
    rpc_url: &str,
    members_bag_id: &str,
    validator_address: SuiAddress,
) -> Result<ResolvedHashiMember> {
    let rpc_method = "suix_getDynamicFieldObject";
    let _timer = JSON_RPC_DURATION
        .with_label_values(&[rpc_method])
        .start_timer();

    let body = sui_rpc_call(
        rpc_url,
        rpc_method,
        serde_json::json!([
            members_bag_id,
            {"type": "address", "value": validator_address.to_string()}
        ]),
    )
    .await?;

    let member_fields = body
        .pointer("/result/data/content/fields/value/fields")
        .with_context(|| {
            format!(
                "missing MemberInfo fields for validator {validator_address} under {members_bag_id}"
            )
        })?;

    // `tls_public_key` is `vector<u8>` in Move; over JSON-RPC it surfaces as an
    // array of numbers.
    let tls_public_key = member_fields
        .get("tls_public_key")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect::<Vec<u8>>()
        })
        .unwrap_or_default();

    Ok(ResolvedHashiMember {
        validator_address,
        tls_public_key,
    })
}

/// Parses Move's `Bag { id: UID { id: <ObjectID> }, size: u64 }` shape from a
/// Sui JSON-RPC display response and returns the inner ObjectID as a String.
fn parse_bag_id(field: Option<&Value>, name: &str) -> Result<String> {
    field
        .and_then(|f| f.pointer("/fields/id/id"))
        .and_then(Value::as_str)
        .map(|s| s.to_owned())
        .with_context(|| format!("could not extract {name}.id from JSON-RPC response"))
}

fn parse_u64_field(field: Option<&Value>, name: &str) -> Result<u64> {
    // Sui JSON-RPC display path serializes u64 as a string to preserve precision.
    field
        .and_then(Value::as_str)
        .with_context(|| format!("missing or non-string {name}"))?
        .parse()
        .with_context(|| format!("parsing {name} as u64"))
}

fn parse_optional_u64(value: &Value) -> Result<Option<u64>> {
    // Move's Option<u64> arrives as either null, or {"vec": ["<n>"]} (some) /
    // {"vec": []} (none) depending on the JSON-RPC display format. Handle both.
    if value.is_null() {
        return Ok(None);
    }
    if let Some(arr) = value.pointer("/vec").and_then(Value::as_array) {
        return match arr.first() {
            Some(v) => {
                let s = v.as_str().context("Option<u64>.vec[0] not a string")?;
                Ok(Some(s.parse().context("parsing Option<u64> inner value")?))
            }
            None => Ok(None),
        };
    }
    // Some display paths render Some(n) as a bare string instead of {"vec":[…]}.
    if let Some(s) = value.as_str() {
        return Ok(Some(s.parse().context("parsing Option<u64> bare value")?));
    }
    bail!("unrecognized Option<u64> JSON shape: {value}")
}

/// JSON-RPC helper that returns the raw response `Value` so callers can
/// pointer-walk into nested fields without imposing a fixed schema (the Move
/// display layout differs per type).
async fn sui_rpc_call(rpc_url: &str, method: &str, params: Value) -> Result<Value> {
    let client = reqwest::Client::builder().build().unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    });
    let response = client
        .post(rpc_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request.to_string())
        .send()
        .await
        .with_context(|| {
            JSON_RPC_STATE
                .with_label_values(&[method, "failed_get"])
                .inc();
            format!("rpc call {method} failed")
        })?;

    let raw = response.bytes().await.with_context(|| {
        JSON_RPC_STATE
            .with_label_values(&[method, "failed_body_extract"])
            .inc();
        format!("unable to read body of {method}")
    })?;

    let body: Value = match serde_json::from_slice(&raw) {
        Ok(v) => v,
        Err(error) => {
            JSON_RPC_STATE
                .with_label_values(&[method, "failed_json_decode"])
                .inc();
            bail!("unable to decode {method} response: {error}: {raw:?}");
        }
    };

    if let Some(error) = body.get("error") {
        JSON_RPC_STATE
            .with_label_values(&[method, "rpc_error"])
            .inc();
        bail!("rpc {method} returned error: {error}");
    }

    JSON_RPC_STATE.with_label_values(&[method, "success"]).inc();
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{CertKeyPair, generate_self_cert};
    use serde::Serialize;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::{BridgeCommitteeSummary, BridgeSummary, MoveTypeCommitteeMember};
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

    #[tokio::test]
    async fn test_extract_bridge_invalid_bridge_url() {
        let summary = BridgeSummary {
            committee: BridgeCommitteeSummary {
                members: vec![(
                    vec![],
                    MoveTypeCommitteeMember {
                        sui_address: SuiAddress::ZERO,
                        http_rest_url: "invalid_bridge_url".as_bytes().to_vec(),
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            ..Default::default()
        };

        let metrics_keys = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut cache = metrics_keys.write().unwrap();
            cache.insert(
                "invalid_bridge_url".to_string(),
                Ed25519PublicKey::from_bytes(&[1u8; 32]).unwrap(),
            );
        }
        let result = extract_bridge(summary, Arc::new(BTreeMap::new()), metrics_keys.clone()).await;

        assert_eq!(
            result.len(),
            0,
            "Should not fall back on cache if invalid bridge url is set"
        );
    }

    #[tokio::test]
    async fn test_extract_bridge_interrupted_response() {
        let summary = BridgeSummary {
            committee: BridgeCommitteeSummary {
                members: vec![(
                    vec![],
                    MoveTypeCommitteeMember {
                        sui_address: SuiAddress::ZERO,
                        http_rest_url: "https://unresponsive_bridge_url".as_bytes().to_vec(),
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            ..Default::default()
        };

        let metrics_keys = Arc::new(RwLock::new(HashMap::new()));
        {
            let mut cache = metrics_keys.write().unwrap();
            cache.insert(
                "https://unresponsive_bridge_url".to_string(),
                Ed25519PublicKey::from_bytes(&[1u8; 32]).unwrap(),
            );
        }
        let result = extract_bridge(summary, Arc::new(BTreeMap::new()), metrics_keys.clone()).await;

        assert_eq!(
            result.len(),
            1,
            "Should fall back on cache if invalid response occurs"
        );
        let allowed_peer = &result[0].1;
        assert_eq!(
            allowed_peer.public_key.as_bytes(),
            &[1u8; 32],
            "Should fall back to the cached public key"
        );

        let cache = metrics_keys.read().unwrap();
        assert!(
            cache.contains_key("https://unresponsive_bridge_url"),
            "Cache should still contain the original key"
        );
    }

    #[test]
    fn test_append_path_segment() {
        let test_cases = vec![
            (
                "https://example.com",
                "metrics_pub_key",
                "https://example.com/metrics_pub_key",
            ),
            (
                "https://example.com/api",
                "metrics_pub_key",
                "https://example.com/api/metrics_pub_key",
            ),
            (
                "https://example.com/",
                "metrics_pub_key",
                "https://example.com/metrics_pub_key",
            ),
            (
                "https://example.com/api/",
                "metrics_pub_key",
                "https://example.com/api/metrics_pub_key",
            ),
            (
                "https://example.com:8080",
                "metrics_pub_key",
                "https://example.com:8080/metrics_pub_key",
            ),
            (
                "https://example.com?param=value",
                "metrics_pub_key",
                "https://example.com/metrics_pub_key?param=value",
            ),
            (
                "https://example.com:8080/api/v1?param=value",
                "metrics_pub_key",
                "https://example.com:8080/api/v1/metrics_pub_key?param=value",
            ),
        ];

        for (input_url, segment, expected_output) in test_cases {
            let url = Url::parse(input_url).unwrap();
            let result = append_path_segment(url, segment);
            assert!(
                result.is_some(),
                "Failed to append segment for URL: {}",
                input_url
            );
            let result_url = result.unwrap();
            assert_eq!(
                result_url.as_str(),
                expected_output,
                "Unexpected result for input URL: {}",
                input_url
            );
        }
    }

    // Hashi resolver tests

    fn addr(byte: u8) -> SuiAddress {
        // Build a deterministic test SuiAddress from a single discriminator byte.
        let mut bytes = [0u8; 32];
        bytes[31] = byte;
        SuiAddress::from_bytes(bytes).unwrap()
    }

    /// Generates a real Ed25519 public key. We can't just use `[byte; 32]` because
    /// not every 32-byte string decompresses to a valid Ed25519 curve point —
    /// `extract_hashi` calls `Ed25519PublicKey::from_bytes` which rejects invalid
    /// points, so test inputs have to be genuine keys.
    fn fresh_pk_bytes() -> Vec<u8> {
        use fastcrypto::ed25519::Ed25519KeyPair;
        use fastcrypto::traits::KeyPair;
        let kp = Ed25519KeyPair::generate(&mut rand::thread_rng());
        kp.public().as_bytes().to_vec()
    }

    #[test]
    fn extract_hashi_keeps_members_with_valid_tls_key() {
        let names: BTreeMap<SuiAddress, String> = [
            (addr(0xAA), "alice".to_string()),
            (addr(0xBB), "bob".to_string()),
        ]
        .into_iter()
        .collect();
        let members = vec![
            ResolvedHashiMember {
                validator_address: addr(0xAA),
                tls_public_key: fresh_pk_bytes(),
            },
            ResolvedHashiMember {
                validator_address: addr(0xBB),
                tls_public_key: fresh_pk_bytes(),
            },
        ];

        let peers = extract_hashi(members, &names);
        assert_eq!(peers.len(), 2);

        let names_out: std::collections::HashSet<_> =
            peers.iter().map(|(_, p)| p.name.clone()).collect();
        assert!(names_out.contains("hashi-alice"));
        assert!(names_out.contains("hashi-bob"));
    }

    #[test]
    fn extract_hashi_skips_members_with_empty_tls_key() {
        // A member that registered but hasn't called set_tls_public_key yet should
        // be silently dropped from the allowlist — they can't authenticate anyway.
        let members = vec![
            ResolvedHashiMember {
                validator_address: addr(0xAA),
                tls_public_key: fresh_pk_bytes(),
            },
            ResolvedHashiMember {
                validator_address: addr(0xBB),
                tls_public_key: vec![], // not yet set
            },
        ];
        let peers = extract_hashi(members, &BTreeMap::new());
        assert_eq!(
            peers.len(),
            1,
            "only the member with a 32-byte key survives"
        );
    }

    #[test]
    fn extract_hashi_skips_members_with_wrong_length_tls_key() {
        // Defensive: an on-chain bug could in principle let a non-32-byte vector
        // through (Move asserts length at set time, but we don't want to depend
        // on Move-side invariants for the proxy's auth correctness).
        let members = vec![ResolvedHashiMember {
            validator_address: addr(0xAA),
            tls_public_key: vec![0x01; 16], // half-size
        }];
        let peers = extract_hashi(members, &BTreeMap::new());
        assert!(peers.is_empty());
    }

    #[test]
    fn extract_hashi_falls_back_to_address_label_when_name_missing() {
        // Members whose validator address isn't in the cached validator-name map
        // (e.g. resolver ran before the sui-validator-set poll finished, or the
        // member's validator entry rotated since) should still be allowed — they're
        // on chain — but labeled by raw address.
        let member_addr = addr(0xCC);
        let members = vec![ResolvedHashiMember {
            validator_address: member_addr,
            tls_public_key: fresh_pk_bytes(),
        }];
        let peers = extract_hashi(members, &BTreeMap::new());
        assert_eq!(peers.len(), 1);
        assert!(
            peers[0].1.name.starts_with("hashi-0x"),
            "expected fallback to address label, got {}",
            peers[0].1.name,
        );
        assert!(peers[0].1.name.contains(&member_addr.to_string()));
    }

    #[test]
    fn extract_hashi_dedups_pubkey_collision() {
        // Two distinct validator_addresses with the same tls_public_key is a
        // pathological case (operators reusing keys); the second insertion into
        // the HashMap downstream of this fn wins. We just verify extract_hashi
        // itself emits both entries and lets the caller's HashMap dedup.
        let shared_key = fresh_pk_bytes();
        let members = vec![
            ResolvedHashiMember {
                validator_address: addr(0xAA),
                tls_public_key: shared_key.clone(),
            },
            ResolvedHashiMember {
                validator_address: addr(0xBB),
                tls_public_key: shared_key,
            },
        ];
        let peers = extract_hashi(members, &BTreeMap::new());
        assert_eq!(peers.len(), 2);
        // Same pubkey → both entries are dropped into the same HashMap key on
        // the consumer side; we confirm shared key here so the test fails loudly
        // if we ever silently change that behavior.
        assert_eq!(peers[0].0, peers[1].0);
    }

    #[test]
    fn parse_bag_id_extracts_inner_id() {
        let v: Value =
            serde_json::from_str(r#"{ "fields": { "id": { "id": "0xdeadbeef" }, "size": "5" } }"#)
                .unwrap();
        let bag_id = parse_bag_id(Some(&v), "test.bag").unwrap();
        assert_eq!(bag_id, "0xdeadbeef");
    }

    #[test]
    fn parse_bag_id_errors_on_missing_field() {
        assert!(parse_bag_id(None, "test.bag").is_err());
        let v: Value = serde_json::json!({"fields": {"size": "5"}});
        assert!(parse_bag_id(Some(&v), "test.bag").is_err());
    }

    #[test]
    fn parse_u64_field_handles_string_encoding() {
        // Sui JSON-RPC display path returns u64 as a string to avoid JS precision loss.
        let v: Value = serde_json::json!("12345");
        assert_eq!(parse_u64_field(Some(&v), "test").unwrap(), 12345);
    }

    #[test]
    fn parse_optional_u64_handles_all_shapes() {
        // null → None
        assert_eq!(parse_optional_u64(&Value::Null).unwrap(), None);
        // {"vec": []} → None  (Move Option<T> empty)
        let none_vec: Value = serde_json::json!({"vec": []});
        assert_eq!(parse_optional_u64(&none_vec).unwrap(), None);
        // {"vec": ["7"]} → Some(7)
        let some_vec: Value = serde_json::json!({"vec": ["7"]});
        assert_eq!(parse_optional_u64(&some_vec).unwrap(), Some(7));
        // Bare "7" → Some(7) (some display paths skip the vec wrapper)
        let bare: Value = serde_json::json!("7");
        assert_eq!(parse_optional_u64(&bare).unwrap(), Some(7));
    }
}
