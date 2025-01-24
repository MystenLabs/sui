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
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Add;
use std::sync::Arc;

use self::metrics::TrafficControllerMetrics;
use crate::traffic_controller::nodefw_client::{BlockAddress, BlockAddresses, NodeFWClient};
use crate::traffic_controller::policies::{
    Policy, PolicyResponse, TrafficControlPolicy, TrafficTally,
};
use mysten_metrics::spawn_monitored_task;
use rand::Rng;
use std::fmt::Debug;
use std::time::{Duration, Instant, SystemTime};
use sui_types::traffic_control::{PolicyConfig, PolicyType, RemoteFirewallConfig, Weight};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, error, info, trace, warn};

pub const METRICS_INTERVAL_SECS: u64 = 2;
pub const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 300;

type Blocklist = Arc<DashMap<IpAddr, SystemTime>>;

#[derive(Clone)]
struct Blocklists {
    clients: Blocklist,
    proxied_clients: Blocklist,
}

#[derive(Clone)]
enum Acl {
    Blocklists(Blocklists),
    /// If this variant is set, then we do no tallying or running
    /// of background tasks, and instead simply block all IPs not
    /// in the allowlist on calls to `check`. The allowlist should
    /// only be populated once at initialization.
    Allowlist(Vec<IpAddr>),
}

#[derive(Clone)]
pub struct TrafficController {
    tally_channel: Option<mpsc::Sender<TrafficTally>>,
    acl: Acl,
    metrics: Arc<TrafficControllerMetrics>,
    dry_run_mode: bool,
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
    pub fn init(
        policy_config: PolicyConfig,
        metrics: TrafficControllerMetrics,
        fw_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        match policy_config.allow_list {
            Some(allow_list) => {
                let allowlist = allow_list
                    .into_iter()
                    .map(|ip_str| {
                        parse_ip(&ip_str).unwrap_or_else(|| {
                            panic!("Failed to parse allowlist IP address: {:?}", ip_str)
                        })
                    })
                    .collect();
                Self {
                    tally_channel: None,
                    acl: Acl::Allowlist(allowlist),
                    metrics: Arc::new(metrics),
                    dry_run_mode: policy_config.dry_run,
                }
            }
            None => Self::spawn(policy_config, metrics, fw_config),
        }
    }

    fn spawn(
        policy_config: PolicyConfig,
        metrics: TrafficControllerMetrics,
        fw_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        let metrics = Arc::new(metrics);
        Self::set_policy_config_metrics(&policy_config, metrics.clone());
        let (tx, rx) = mpsc::channel(policy_config.channel_capacity);
        // Memoized drainfile existence state. This is passed into delegation
        // funtions to prevent them from continuing to populate blocklists
        // if drain is set, as otherwise it will grow without bounds
        // without the firewall running to periodically clear it.
        let mem_drainfile_present = fw_config
            .as_ref()
            .map(|config| config.drain_path.exists())
            .unwrap_or(false);
        metrics
            .deadmans_switch_enabled
            .set(mem_drainfile_present as i64);
        let blocklists = Blocklists {
            clients: Arc::new(DashMap::new()),
            proxied_clients: Arc::new(DashMap::new()),
        };
        let tally_loop_blocklists = blocklists.clone();
        let clear_loop_blocklists = blocklists.clone();
        let tally_loop_metrics = metrics.clone();
        let clear_loop_metrics = metrics.clone();
        let dry_run_mode = policy_config.dry_run;
        spawn_monitored_task!(run_tally_loop(
            rx,
            policy_config,
            fw_config,
            tally_loop_blocklists,
            tally_loop_metrics,
            mem_drainfile_present,
        ));
        spawn_monitored_task!(run_clear_blocklists_loop(
            clear_loop_blocklists,
            clear_loop_metrics,
        ));
        Self {
            tally_channel: Some(tx),
            acl: Acl::Blocklists(blocklists),
            metrics: metrics.clone(),
            dry_run_mode,
        }
    }

    fn set_policy_config_metrics(
        policy_config: &PolicyConfig,
        metrics: Arc<TrafficControllerMetrics>,
    ) {
        if let PolicyType::FreqThreshold(config) = &policy_config.spam_policy_type {
            metrics
                .spam_client_threshold
                .set(config.client_threshold as i64);
            metrics
                .spam_proxied_client_threshold
                .set(config.proxied_client_threshold as i64);
        }
        if let PolicyType::FreqThreshold(config) = &policy_config.error_policy_type {
            metrics
                .error_client_threshold
                .set(config.client_threshold as i64);
            metrics
                .error_proxied_client_threshold
                .set(config.proxied_client_threshold as i64);
        }
    }

    pub fn init_for_test(
        policy_config: PolicyConfig,
        fw_config: Option<RemoteFirewallConfig>,
    ) -> Self {
        let metrics = TrafficControllerMetrics::new(&prometheus::Registry::new());
        Self::init(policy_config, metrics, fw_config)
    }

    pub fn tally(&self, tally: TrafficTally) {
        if let Some(channel) = self.tally_channel.as_ref() {
            // Use try_send rather than send mainly to avoid creating backpressure
            // on the caller if the channel is full, which may slow down the critical
            // path. Dropping the tally on the floor should be ok, as in this case
            // we are effectively sampling traffic, which we would need to do anyway
            // if we are overloaded
            match channel.try_send(tally) {
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
    }

    /// Handle check with dry-run mode considered
    pub async fn check(&self, client: &Option<IpAddr>, proxied_client: &Option<IpAddr>) -> bool {
        let check_with_dry_run_maybe = |allowed| -> bool {
            match (allowed, self.dry_run_mode()) {
                // check succeeded
                (true, _) => true,
                // check failed while in dry-run mode
                (false, true) => {
                    debug!("Dry run mode: Blocked request from client {:?}", client);
                    self.metrics.num_dry_run_blocked_requests.inc();
                    true
                }
                // check failed
                (false, false) => false,
            }
        };

        match &self.acl {
            Acl::Allowlist(allowlist) => {
                let allowed = client.is_none() || allowlist.contains(&client.unwrap());
                check_with_dry_run_maybe(allowed)
            }
            Acl::Blocklists(blocklists) => {
                let allowed = self
                    .check_blocklists(blocklists, client, proxied_client)
                    .await;
                check_with_dry_run_maybe(allowed)
            }
        }
    }

    /// Returns true if the connection is in blocklist, false otherwise
    async fn check_blocklists(
        &self,
        blocklists: &Blocklists,
        client: &Option<IpAddr>,
        proxied_client: &Option<IpAddr>,
    ) -> bool {
        let client_check = self.check_and_clear_blocklist(
            client,
            blocklists.clients.clone(),
            &self.metrics.connection_ip_blocklist_len,
        );
        let proxied_client_check = self.check_and_clear_blocklist(
            proxied_client,
            blocklists.proxied_clients.clone(),
            &self.metrics.proxy_ip_blocklist_len,
        );
        let (client_check, proxied_client_check) =
            futures::future::join(client_check, proxied_client_check).await;
        client_check && proxied_client_check
    }

    pub fn dry_run_mode(&self) -> bool {
        self.dry_run_mode
    }

    async fn check_and_clear_blocklist(
        &self,
        client: &Option<IpAddr>,
        blocklist: Blocklist,
        blocklist_len_gauge: &IntGauge,
    ) -> bool {
        let client = match client {
            Some(client) => client,
            None => return true,
        };
        let now = SystemTime::now();
        // the below two blocks cannot be nested, otherwise we will deadlock
        // due to aquiring the lock on get, then holding across the remove
        let (should_block, should_remove) = {
            match blocklist.get(client) {
                Some(expiration) if now >= *expiration => (false, true),
                None => (false, false),
                _ => (true, false),
            }
        };
        if should_remove {
            blocklist_len_gauge.dec();
            blocklist.remove(client);
        }
        !should_block
    }
}

/// Although we clear IPs from the blocklist lazily when they are checked,
/// it's possible that over time we may accumulate a large number of stale
/// IPs in the blocklist for clients that are added, then once blocked,
/// never checked again. This function runs periodically to clear out any
/// such stale IPs. This also ensures that the blocklist length metric
/// accurately reflects TTL.
async fn run_clear_blocklists_loop(blocklists: Blocklists, metrics: Arc<TrafficControllerMetrics>) {
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let now = SystemTime::now();
        blocklists.clients.retain(|_, expiration| now < *expiration);
        blocklists
            .proxied_clients
            .retain(|_, expiration| now < *expiration);
        metrics
            .connection_ip_blocklist_len
            .set(blocklists.clients.len() as i64);
        metrics
            .proxy_ip_blocklist_len
            .set(blocklists.proxied_clients.len() as i64);
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
        .unwrap_or(DEFAULT_DRAIN_TIMEOUT_SECS);
    let mut metric_timer = Instant::now();

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
                    error!("No traffic tallies received in {} seconds.", timeout);
                    if mem_drainfile_present {
                        continue;
                    }
                    if !fw_config.drain_path.exists() {
                        mem_drainfile_present = true;
                        warn!("Draining Node firewall.");
                        File::create(&fw_config.drain_path)
                            .expect("Failed to touch nodefw drain file");
                        metrics.deadmans_switch_enabled.set(1);
                    }
                }
            }
        }

        // every N seconds, we update metrics and logging that would be too
        // spammy to be handled while processing each tally
        if metric_timer.elapsed() > Duration::from_secs(METRICS_INTERVAL_SECS) {
            if let TrafficControlPolicy::FreqThreshold(spam_policy) = &spam_policy {
                if let Some(highest_direct_rate) = spam_policy.highest_direct_rate() {
                    metrics
                        .highest_direct_spam_rate
                        .set(highest_direct_rate.0 as i64);
                    debug!("Recent highest direct spam rate: {:?}", highest_direct_rate);
                }
                if let Some(highest_proxied_rate) = spam_policy.highest_proxied_rate() {
                    metrics
                        .highest_proxied_spam_rate
                        .set(highest_proxied_rate.0 as i64);
                    debug!(
                        "Recent highest proxied spam rate: {:?}",
                        highest_proxied_rate
                    );
                }
            }
            if let TrafficControlPolicy::FreqThreshold(error_policy) = &error_policy {
                if let Some(highest_direct_rate) = error_policy.highest_direct_rate() {
                    metrics
                        .highest_direct_error_rate
                        .set(highest_direct_rate.0 as i64);
                    debug!(
                        "Recent highest direct error rate: {:?}",
                        highest_direct_rate
                    );
                }
                if let Some(highest_proxied_rate) = error_policy.highest_proxied_rate() {
                    metrics
                        .highest_proxied_error_rate
                        .set(highest_proxied_rate.0 as i64);
                    debug!(
                        "Recent highest proxied error rate: {:?}",
                        highest_proxied_rate
                    );
                }
            }
            metric_timer = Instant::now();
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
    let Some((error_weight, error_type)) = tally.clone().error_info else {
        return Ok(());
    };
    if !error_weight.is_sampled() {
        return Ok(());
    }
    trace!(
        "Handling error_type {:?} from client {:?}",
        error_type,
        tally.direct,
    );
    metrics
        .tally_error_types
        .with_label_values(&[error_type.as_str()])
        .inc();
    let resp = policy.handle_tally(tally);
    metrics.error_tally_handled.inc();
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
    if !(tally.spam_weight.is_sampled() && policy_config.spam_sample_rate.is_sampled()) {
        return Ok(());
    }
    let resp = policy.handle_tally(tally.clone());
    metrics.tally_handled.inc();
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
        block_client,
        block_proxied_client,
    } = response;
    let PolicyConfig {
        connection_blocklist_ttl_sec,
        proxy_blocklist_ttl_sec,
        ..
    } = policy_config;
    if let Some(client) = block_client {
        if blocklists
            .clients
            .insert(
                client,
                SystemTime::now() + Duration::from_secs(*connection_blocklist_ttl_sec),
            )
            .is_none()
        {
            // Only increment the metric if the client was not already blocked
            debug!("Blocking client: {:?}", client);
            metrics.requests_blocked_at_protocol.inc();
            metrics.connection_ip_blocklist_len.inc();
        }
    }
    if let Some(client) = block_proxied_client {
        if blocklists
            .proxied_clients
            .insert(
                client,
                SystemTime::now() + Duration::from_secs(*proxy_blocklist_ttl_sec),
            )
            .is_none()
        {
            // Only increment the metric if the client was not already blocked
            debug!("Blocking proxied client: {:?}", client);
            metrics.requests_blocked_at_protocol.inc();
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
        block_client,
        block_proxied_client,
    } = response;
    let PolicyConfig {
        connection_blocklist_ttl_sec,
        proxy_blocklist_ttl_sec,
        ..
    } = policy_config;
    let mut addresses = vec![];
    if let Some(client_id) = block_client {
        debug!("Delegating client blocking to firewall");
        addresses.push(BlockAddress {
            source_address: client_id.to_string(),
            destination_port,
            ttl: *connection_blocklist_ttl_sec,
        });
    }
    if let Some(ip) = block_proxied_client {
        debug!("Delegating proxied client blocking to firewall");
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

#[derive(Debug, Clone)]
pub struct TrafficSimMetrics {
    pub num_requests: u64,
    pub num_blocked: u64,
    pub time_to_first_block: Option<Duration>,
    pub abs_time_to_first_block: Option<Duration>,
    pub total_time_blocked: Duration,
    pub num_blocklist_adds: u64,
}

impl Default for TrafficSimMetrics {
    fn default() -> Self {
        Self {
            num_requests: 0,
            num_blocked: 0,
            time_to_first_block: None,
            abs_time_to_first_block: None,
            total_time_blocked: Duration::from_micros(0),
            num_blocklist_adds: 0,
        }
    }
}

impl Add for TrafficSimMetrics {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            num_requests: self.num_requests + other.num_requests,
            num_blocked: self.num_blocked + other.num_blocked,
            time_to_first_block: match (self.time_to_first_block, other.time_to_first_block) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
            abs_time_to_first_block: match (
                self.abs_time_to_first_block,
                other.abs_time_to_first_block,
            ) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
            total_time_blocked: self.total_time_blocked + other.total_time_blocked,
            num_blocklist_adds: self.num_blocklist_adds + other.num_blocklist_adds,
        }
    }
}

pub struct TrafficSim {
    pub traffic_controller: TrafficController,
}

impl TrafficSim {
    pub async fn run(
        policy: PolicyConfig,
        num_clients: u8,
        per_client_tps: usize,
        duration: Duration,
        report: bool,
    ) -> TrafficSimMetrics {
        assert!(
            per_client_tps <= 10_000,
            "per_client_tps must be less than 10,000. For higher values, increase num_clients"
        );
        assert!(num_clients < 20, "num_clients must be greater than 0");
        assert!(num_clients > 0);
        assert!(per_client_tps > 0);
        assert!(duration.as_secs() > 0);

        let controller = TrafficController::init_for_test(policy.clone(), None);
        let tasks = (0..num_clients).map(|task_num| {
            tokio::spawn(Self::run_single_client(
                controller.clone(),
                duration,
                task_num,
                per_client_tps,
            ))
        });

        let status_task = if report {
            Some(tokio::spawn(async move {
                println!(
                    "Running naive traffic simulation for {} seconds",
                    duration.as_secs()
                );
                println!("Policy: {:#?}", policy);
                println!("Num clients: {}", num_clients);
                println!("TPS per client: {}", per_client_tps);
                println!(
                    "Target total TPS: {}",
                    per_client_tps * num_clients as usize
                );
                println!("\n");
                for _ in 0..duration.as_secs() {
                    print!(".");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                println!();
            }))
        } else {
            None
        };

        let metrics = futures::future::join_all(tasks).await.into_iter().fold(
            TrafficSimMetrics::default(),
            |acc, run_client_ret| {
                if run_client_ret.is_err() {
                    error!(
                        "Error running traffic sim client: {:?}",
                        run_client_ret.err()
                    );
                    acc
                } else {
                    let metrics = run_client_ret.unwrap();
                    acc + metrics
                }
            },
        );

        if report {
            status_task.unwrap().await.unwrap();
            Self::report_metrics(metrics.clone(), duration, per_client_tps, num_clients);
        }
        metrics
    }

    async fn run_single_client(
        controller: TrafficController,
        duration: Duration,
        task_num: u8,
        per_client_tps: usize,
    ) -> TrafficSimMetrics {
        // Do an initial sleep for a random amount of time to smooth
        // out the traffic. This shouldn't be strictly necessary and
        // we can remove if we want more determinism
        let sleep_time = Duration::from_micros(rand::thread_rng().gen_range(0..100));
        tokio::time::sleep(sleep_time).await;

        // collectors
        let mut num_requests = 0;
        let mut num_blocked = 0;
        let mut time_to_first_block = None;
        let mut total_time_blocked = Duration::from_micros(0);
        let mut num_blocklist_adds = 0;
        // state variables
        let mut currently_blocked = false;
        let mut time_blocked_start = Instant::now();
        let start = Instant::now();

        while start.elapsed() < duration {
            let client = Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, task_num)));
            let allowed = controller.check(&client, &None).await;
            if allowed {
                if currently_blocked {
                    total_time_blocked += time_blocked_start.elapsed();
                    currently_blocked = false;
                }
                controller.tally(TrafficTally::new(
                    client,
                    // TODO add proxy IP for testing
                    None,
                    // TODO add weight adjustments
                    None,
                    Weight::one(),
                ));
            } else {
                if !currently_blocked {
                    time_blocked_start = Instant::now();
                    currently_blocked = true;
                    num_blocklist_adds += 1;
                    if time_to_first_block.is_none() {
                        time_to_first_block = Some(start.elapsed());
                    }
                }
                num_blocked += 1;
            }
            num_requests += 1;
            tokio::time::sleep(Duration::from_micros(1_000_000 / per_client_tps as u64)).await;
        }
        TrafficSimMetrics {
            num_requests,
            num_blocked,
            time_to_first_block,
            abs_time_to_first_block: time_to_first_block,
            total_time_blocked,
            num_blocklist_adds,
        }
    }

    fn report_metrics(
        metrics: TrafficSimMetrics,
        duration: Duration,
        per_client_tps: usize,
        num_clients: u8,
    ) {
        println!("TrafficSim metrics:");
        println!("-------------------");
        // The below two should be near equal
        println!(
            "Num expected requests: {}",
            per_client_tps * (num_clients as usize) * duration.as_secs() as usize
        );
        println!("Num actual requests: {}", metrics.num_requests);
        // This reflects the number of requests that were blocked, but note that once a client
        // is added to the blocklist, all subsequent requests from that client are blocked
        // until ttl is expired.
        println!("Num blocked requests: {}", metrics.num_blocked);
        // This metric on the other hand reflects the number of times a client was added to the blocklist
        // and thus can be compared with the expectation based on the policy block threshold and ttl
        println!(
            "Num times added to blocklist: {}",
            metrics.num_blocklist_adds
        );
        // This averages the duration for the first request to be blocked across all clients,
        // which is useful for understanding if the policy is rate limiting based on expectation
        let avg_first_block_time = metrics
            .time_to_first_block
            .map(|ttf| ttf / num_clients as u32);
        println!("Average time to first block: {:?}", avg_first_block_time);
        // This is the time it took for the first request to be blocked across all clients,
        // and is instead more useful for understanding false positives in terms of rate and magnitude.
        println!(
            "Abolute time to first block (across all clients): {:?}",
            metrics.abs_time_to_first_block
        );
        // Useful for ensuring that TTL is respected
        let avg_time_blocked = if metrics.num_blocklist_adds > 0 {
            metrics.total_time_blocked.as_millis() as u64 / metrics.num_blocklist_adds
        } else {
            0
        };
        println!(
            "Average time blocked (ttl): {:?}",
            Duration::from_millis(avg_time_blocked)
        );
    }
}

pub fn parse_ip(ip: &str) -> Option<IpAddr> {
    ip.parse::<IpAddr>().ok().or_else(|| {
        ip.parse::<SocketAddr>()
            .ok()
            .map(|socket_addr| socket_addr.ip())
            .or_else(|| {
                error!("Failed to parse value of {:?} to ip address or socket.", ip,);
                None
            })
    })
}
