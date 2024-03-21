// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod metrics;
pub mod nodefw_client;
#[cfg(debug_assertions)]
pub mod nodefw_test_server;
pub mod policies;

use std::net::{IpAddr, SocketAddr};
use std::{collections::HashMap, sync::Arc};

use self::metrics::TrafficControllerMetrics;
use crate::traffic_controller::nodefw_client::{BlockAddress, BlockAddresses, NodeFWClient};
use crate::traffic_controller::policies::{
    Policy, PolicyResponse, TrafficControlPolicy, TrafficTally,
};
use jsonrpsee::types::error::ErrorCode;
use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use std::fmt::Debug;
use std::time::{Duration, SystemTime};
use sui_types::error::SuiError;
use sui_types::traffic_control::{PolicyConfig, RemoteFirewallConfig, ServiceResponse};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, warn};

type BlocklistT = Arc<RwLock<HashMap<IpAddr, SystemTime>>>;

#[derive(Clone)]
struct Blocklists {
    connection_ips: BlocklistT,
    proxy_ips: BlocklistT,
}

#[derive(Clone)]
pub struct TrafficController {
    tally_channel: mpsc::Sender<TrafficTally>,
    blocklists: Blocklists,
    metrics: Arc<TrafficControllerMetrics>,
}

impl Debug for TrafficController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: we do not want to print the contents of the blocklists to logs
        // given that (1) it contains all requests IPs, and (2) it could be quite
        // large. Instead, we print lengths of the blocklists. Further, we prefer
        // to get length from the metrics rather than from the blocklists themselves
        // to avoid unneccesarily aquiring the read lock.
        f.debug_struct("TrafficController")
            .field(
                "connection_ip_blocklist_len",
                &self.metrics.connection_ip_blocklist_len.get(),
            )
            .field(
                "proxy_ip_blocklist_len",
                &self.metrics.proxy_ip_blocklist_len.get(),
            )
            .finish()
    }
}

impl TrafficController {
    pub async fn spawn(
        fw_config: RemoteFirewallConfig,
        policy_config: PolicyConfig,
        metrics: TrafficControllerMetrics,
    ) -> Self {
        let metrics = Arc::new(metrics);
        let (tx, rx) = mpsc::channel(policy_config.channel_capacity);
        let ret = Self {
            tally_channel: tx,
            blocklists: Blocklists {
                connection_ips: Arc::new(RwLock::new(HashMap::new())),
                proxy_ips: Arc::new(RwLock::new(HashMap::new())),
            },
            metrics: metrics.clone(),
        };
        let blocklists = ret.blocklists.clone();
        spawn_monitored_task!(run_tally_loop(
            rx,
            policy_config,
            fw_config,
            blocklists,
            metrics
        ));
        ret
    }

    pub fn tally(&self, tally: TrafficTally) {
        // Use try_send rather than send mainly to avoid creating backpressure
        // on the caller if the channel is full, which may slow down the critical
        // path. Dropping the tally on the floor should be ok, as in this case
        // we are effectively sampling traffic, which we would need to do anyway
        // if we are overloaded
        match self.tally_channel.try_send(tally) {
            Err(TrySendError::Full(_)) => {
                warn!("TrafficController tally channel full, dropping tally");
                // TODO: metric
            }
            Err(TrySendError::Closed(_)) => {
                panic!("TrafficController tally channel closed unexpectedly");
            }
            Ok(_) => {}
        }
    }

    /// Returns true if the connection is allowed, false if it is blocked
    pub async fn check(
        &self,
        connection_ip: Option<SocketAddr>,
        proxy_ip: Option<SocketAddr>,
    ) -> bool {
        match (connection_ip, proxy_ip) {
            (Some(connection_ip), _) => self.check_and_clear_blocklist(connection_ip, true).await,
            (_, Some(proxy_ip)) => self.check_and_clear_blocklist(proxy_ip, false).await,
            _ => true,
        }
    }

    async fn check_and_clear_blocklist(&self, addr: SocketAddr, connection_ips: bool) -> bool {
        let ip = addr.ip();
        let (blocklist, metric_gauge) = if connection_ips {
            (
                self.blocklists.connection_ips.clone(),
                &self.metrics.connection_ip_blocklist_len,
            )
        } else {
            (
                self.blocklists.proxy_ips.clone(),
                &self.metrics.proxy_ip_blocklist_len,
            )
        };

        let now = SystemTime::now();
        let expiration = blocklist.read().get(&ip).copied();
        match expiration {
            Some(expiration) if now >= expiration => {
                metric_gauge.dec();
                blocklist.write().remove(&ip);
                true
            }
            None => true,
            _ => {
                self.metrics.requests_blocked_at_protocol.inc();
                false
            }
        }
    }
}

// TODO: Needs thorough testing/auditing before this can be used in error policy
//
/// Errors that are tallied and can be used to determine if a request should be blocked.
fn is_tallyable_error(response: &ServiceResponse) -> bool {
    match response {
        ServiceResponse::Validator(Err(err)) => {
            matches!(
                err,
                SuiError::UserInputError { .. }
                    | SuiError::InvalidSignature { .. }
                    | SuiError::SignerSignatureAbsent { .. }
                    | SuiError::SignerSignatureNumberMismatch { .. }
                    | SuiError::IncorrectSigner { .. }
                    | SuiError::UnknownSigner { .. }
                    | SuiError::WrongEpoch { .. }
            )
        }
        ServiceResponse::Fullnode(resp) => {
            matches!(
                resp.error_code.map(ErrorCode::from),
                Some(ErrorCode::InvalidRequest) | Some(ErrorCode::InvalidParams)
            )
        }

        _ => false,
    }
}

async fn run_tally_loop(
    mut receiver: mpsc::Receiver<TrafficTally>,
    policy_config: PolicyConfig,
    fw_config: RemoteFirewallConfig,
    blocklists: Blocklists,
    metrics: Arc<TrafficControllerMetrics>,
) {
    let mut spam_policy = TrafficControlPolicy::from_spam_config(policy_config.clone()).await;
    let mut error_policy = TrafficControlPolicy::from_error_config(policy_config.clone()).await;
    let spam_blocklists = Arc::new(blocklists.clone());
    let error_blocklists = Arc::new(blocklists);
    let node_fw_client = if !(fw_config.delegate_spam_blocking || fw_config.delegate_error_blocking)
    {
        None
    } else {
        Some(NodeFWClient::new(fw_config.remote_fw_url.clone()))
    };

    loop {
        tokio::select! {
            received = receiver.recv() => match received {
                Some(tally) => {
                    if let Err(err) = handle_spam_tally(
                        &mut spam_policy,
                        &policy_config,
                        &node_fw_client,
                        &fw_config,
                        tally.clone(),
                        spam_blocklists.clone(),
                        metrics.clone(),
                    )
                    .await {
                        warn!("Error handling spam tally: {}", err);
                    }

                    if let Err(err) = handle_error_tally(
                        &mut error_policy,
                        &policy_config,
                        &node_fw_client,
                        &fw_config,
                        tally,
                        error_blocklists.clone(),
                        metrics.clone(),
                    )
                    .await {
                        warn!("Error handling error tally: {}", err);
                    }
                }
                None => {
                    panic!("TrafficController tally channel closed unexpectedly");
                },
            }
        }
    }
}

async fn handle_error_tally(
    policy: &mut TrafficControlPolicy,
    policy_config: &PolicyConfig,
    nodefw_client: &Option<NodeFWClient>,
    fw_config: &RemoteFirewallConfig,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
    metrics: Arc<TrafficControllerMetrics>,
) -> Result<(), reqwest::Error> {
    if tally.result.is_ok() {
        return Ok(());
    }
    if !is_tallyable_error(&tally.result) {
        return Ok(());
    }
    let resp = policy.handle_tally(tally.clone());
    if fw_config.delegate_error_blocking {
        let client = nodefw_client
            .as_ref()
            .expect("Expected NodeFWClient for blocklist delegation");
        delegate_policy_response(
            resp,
            policy_config,
            client,
            fw_config.destination_port,
            metrics.clone(),
        )
        .await
    } else {
        handle_policy_response(resp, policy_config, blocklists, metrics).await;
        Ok(())
    }
}

async fn handle_spam_tally(
    policy: &mut TrafficControlPolicy,
    policy_config: &PolicyConfig,
    nodefw_client: &Option<NodeFWClient>,
    fw_config: &RemoteFirewallConfig,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
    metrics: Arc<TrafficControllerMetrics>,
) -> Result<(), reqwest::Error> {
    let resp = policy.handle_tally(tally.clone());
    if fw_config.delegate_spam_blocking {
        let client = nodefw_client
            .as_ref()
            .expect("Expected NodeFWClient for blocklist delegation");
        delegate_policy_response(
            resp,
            policy_config,
            client,
            fw_config.destination_port,
            metrics.clone(),
        )
        .await
    } else {
        handle_policy_response(resp, policy_config, blocklists.clone(), metrics).await;
        Ok(())
    }
}

async fn handle_policy_response(
    response: PolicyResponse,
    policy_config: &PolicyConfig,
    blocklists: Arc<Blocklists>,
    metrics: Arc<TrafficControllerMetrics>,
) {
    let PolicyResponse {
        block_connection_ip,
        block_proxy_ip,
    } = response;
    let PolicyConfig {
        connection_blocklist_ttl_sec,
        proxy_blocklist_ttl_sec,
        ..
    } = policy_config;
    if let Some(ip) = block_connection_ip {
        if blocklists
            .connection_ips
            .write()
            .insert(
                ip,
                SystemTime::now() + Duration::from_secs(*connection_blocklist_ttl_sec),
            )
            .is_none()
        {
            // Only increment the metric if the IP was not already blocked
            debug!("Blocking connection IP");
            metrics.connection_ip_blocklist_len.inc();
        }
    }
    if let Some(ip) = block_proxy_ip {
        if blocklists
            .proxy_ips
            .write()
            .insert(
                ip,
                SystemTime::now() + Duration::from_secs(*proxy_blocklist_ttl_sec),
            )
            .is_none()
        {
            // Only increment the metric if the IP was not already blocked
            debug!("Blocking proxy IP");
            metrics.proxy_ip_blocklist_len.inc();
        }
    }
}

async fn delegate_policy_response(
    response: PolicyResponse,
    policy_config: &PolicyConfig,
    node_fw_client: &NodeFWClient,
    destination_port: u16,
    metrics: Arc<TrafficControllerMetrics>,
) -> Result<(), reqwest::Error> {
    let PolicyResponse {
        block_connection_ip,
        block_proxy_ip,
    } = response;
    let PolicyConfig {
        connection_blocklist_ttl_sec,
        proxy_blocklist_ttl_sec,
        ..
    } = policy_config;
    let mut addresses = vec![];
    if let Some(ip) = block_connection_ip {
        debug!("Delegating connection IP blocking to firewall");
        addresses.push(BlockAddress {
            source_address: ip.to_string(),
            destination_port,
            ttl: *connection_blocklist_ttl_sec,
        });
    }
    if let Some(ip) = block_proxy_ip {
        debug!("Delegating proxy IP blocking to firewall");
        addresses.push(BlockAddress {
            source_address: ip.to_string(),
            destination_port,
            ttl: *proxy_blocklist_ttl_sec,
        });
    }
    if addresses.is_empty() {
        Ok(())
    } else {
        metrics
            .blocks_delegated_to_firewall
            .inc_by(addresses.len() as u64);
        match node_fw_client
            .block_addresses(BlockAddresses { addresses })
            .await
        {
            Ok(()) => Ok(()),
            Err(err) => {
                metrics.firewall_delegation_request_fail.inc();
                Err(err)
            }
        }
    }
}
