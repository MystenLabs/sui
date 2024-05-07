// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod metrics;
pub mod nodefw_client;
pub mod nodefw_test_server;
pub mod policies;

use dashmap::DashMap;
use fs::File;
use prometheus::IntGauge;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use self::metrics::TrafficControllerMetrics;
use crate::traffic_controller::nodefw_client::{BlockAddress, BlockAddresses, NodeFWClient};
use crate::traffic_controller::policies::{
    Policy, PolicyResponse, TrafficControlPolicy, TrafficTally,
};
use jsonrpsee::types::error::ErrorCode;
use mysten_metrics::spawn_monitored_task;
use std::fmt::Debug;
use std::time::{Duration, SystemTime};
use sui_types::error::SuiError;
use sui_types::traffic_control::{PolicyConfig, RemoteFirewallConfig, ServiceResponse};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, error, info, warn};

type BlocklistT = Arc<DashMap<IpAddr, SystemTime>>;

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
    pub fn spawn(
        policy_config: PolicyConfig,
        metrics: TrafficControllerMetrics,
        fw_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        let metrics = Arc::new(metrics);
        let (tx, rx) = mpsc::channel(policy_config.channel_capacity);
        // Memoized drainfile existence state. This is passed into delegation
        // funtions to prevent them from continuing to populate blocklists
        // if drain is set, as otherwise it will grow without bounds
        // without the firewall running to periodically clear it.
        let mem_drainfile_present = fw_config
            .as_ref()
            .map(|config| config.drain_path.exists())
            .unwrap_or(false);

        let ret = Self {
            tally_channel: tx,
            blocklists: Blocklists {
                connection_ips: Arc::new(DashMap::new()),
                proxy_ips: Arc::new(DashMap::new()),
            },
            metrics: metrics.clone(),
        };
        let blocklists = ret.blocklists.clone();
        spawn_monitored_task!(run_tally_loop(
            rx,
            policy_config,
            fw_config,
            blocklists,
            metrics,
            mem_drainfile_present,
        ));
        ret
    }

    pub fn spawn_for_test(
        policy_config: PolicyConfig,
        fw_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        let metrics = TrafficControllerMetrics::new(&prometheus::Registry::new());
        Self::spawn(policy_config, metrics, fw_config)
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
                self.metrics.tally_channel_overflow.inc();
                // TODO: once we've verified this doesn't happen under normal
                // conditions, we can consider dropping the request itself given
                // that clearly the system is overloaded
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
        let connection_check = self.check_and_clear_blocklist(
            connection_ip,
            self.blocklists.connection_ips.clone(),
            &self.metrics.connection_ip_blocklist_len,
        );
        let proxy_check = self.check_and_clear_blocklist(
            proxy_ip,
            self.blocklists.proxy_ips.clone(),
            &self.metrics.proxy_ip_blocklist_len,
        );
        let (conn_check, proxy_check) = futures::future::join(connection_check, proxy_check).await;
        conn_check && proxy_check
    }

    async fn check_and_clear_blocklist(
        &self,
        ip: Option<SocketAddr>,
        blocklist: BlocklistT,
        metric_gauge: &IntGauge,
    ) -> bool {
        let ip = match ip {
            Some(ip) => ip,
            None => return true,
        }
        .ip();
        let now = SystemTime::now();
        match blocklist.get(&ip) {
            Some(expiration) if now >= *expiration => {
                metric_gauge.dec();
                blocklist.remove(&ip);
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
    fw_config: Option<RemoteFirewallConfig>,
    blocklists: Blocklists,
    metrics: Arc<TrafficControllerMetrics>,
    mut mem_drainfile_present: bool,
) {
    let mut spam_policy = TrafficControlPolicy::from_spam_config(policy_config.clone()).await;
    let mut error_policy = TrafficControlPolicy::from_error_config(policy_config.clone()).await;
    let spam_blocklists = Arc::new(blocklists.clone());
    let error_blocklists = Arc::new(blocklists);
    let node_fw_client = fw_config
        .as_ref()
        .map(|fw_config| NodeFWClient::new(fw_config.remote_fw_url.clone()));

    let timeout = fw_config
        .as_ref()
        .map(|fw_config| fw_config.drain_timeout_secs)
        .unwrap_or(300);

    loop {
        tokio::select! {
            received = receiver.recv() => {
                metrics.tallies.inc();
                match received {
                    Some(tally) => {
                        // TODO: spawn a task to handle tallying concurrently
                        if let Err(err) = handle_spam_tally(
                            &mut spam_policy,
                            &policy_config,
                            &node_fw_client,
                            &fw_config,
                            tally.clone(),
                            spam_blocklists.clone(),
                            metrics.clone(),
                            mem_drainfile_present,
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
                            mem_drainfile_present,
                        )
                        .await {
                            warn!("Error handling error tally: {}", err);
                        }
                    }
                    None => {
                        info!("TrafficController tally channel closed by all senders");
                        return;
                    }
                }
            }
            // Dead man's switch - if we suspect something is sinking all traffic to node, disable nodefw
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(timeout)) => {
                if let Some(fw_config) = &fw_config {
                    error!("No traffic tallies received in {} seconds.", fw_config.drain_timeout_secs);
                    if mem_drainfile_present {
                        continue;
                    }
                    if !fw_config.drain_path.exists() {
                        mem_drainfile_present = true;
                        warn!("Draining Node firewall.");
                        File::create(&fw_config.drain_path)
                            .expect("Failed to touch nodefw drain file");
                    }
                }
            }
        }
    }
}

async fn handle_error_tally(
    policy: &mut TrafficControlPolicy,
    policy_config: &PolicyConfig,
    nodefw_client: &Option<NodeFWClient>,
    fw_config: &Option<RemoteFirewallConfig>,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
    metrics: Arc<TrafficControllerMetrics>,
    mem_drainfile_present: bool,
) -> Result<(), reqwest::Error> {
    if tally.result.is_ok() {
        return Ok(());
    }
    if !is_tallyable_error(&tally.result) {
        return Ok(());
    }
    let resp = policy.handle_tally(tally.clone());
    if let Some(fw_config) = fw_config {
        if fw_config.delegate_error_blocking && !mem_drainfile_present {
            let client = nodefw_client
                .as_ref()
                .expect("Expected NodeFWClient for blocklist delegation");
            return delegate_policy_response(
                resp,
                policy_config,
                client,
                fw_config.destination_port,
                metrics.clone(),
            )
            .await;
        }
    }
    handle_policy_response(resp, policy_config, blocklists, metrics).await;
    Ok(())
}

async fn handle_spam_tally(
    policy: &mut TrafficControlPolicy,
    policy_config: &PolicyConfig,
    nodefw_client: &Option<NodeFWClient>,
    fw_config: &Option<RemoteFirewallConfig>,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
    metrics: Arc<TrafficControllerMetrics>,
    mem_drainfile_present: bool,
) -> Result<(), reqwest::Error> {
    let resp = policy.handle_tally(tally.clone());
    if let Some(fw_config) = fw_config {
        if fw_config.delegate_spam_blocking && !mem_drainfile_present {
            let client = nodefw_client
                .as_ref()
                .expect("Expected NodeFWClient for blocklist delegation");
            return delegate_policy_response(
                resp,
                policy_config,
                client,
                fw_config.destination_port,
                metrics.clone(),
            )
            .await;
        }
    }
    handle_policy_response(resp, policy_config, blocklists, metrics).await;
    Ok(())
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
