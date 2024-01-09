// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use mysten_metrics::spawn_monitored_task;
use std::time::{Duration, SystemTime};
use sui_types::traffic_control::{
    Policy, PolicyConfig, PolicyResponse, TrafficControlPolicy, TrafficTally,
};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{info, warn};

type BlocklistT = Arc<DashMap<SocketAddr, SystemTime>>;

#[derive(Clone)]
struct Blocklists {
    connection_ips: BlocklistT,
    proxy_ips: BlocklistT,
}

#[derive(Clone)]
pub struct TrafficController {
    tally_channel: mpsc::Sender<TrafficTally>,
    blocklists: Blocklists,
    //metrics: TrafficControllerMetrics, // TODO
}

impl TrafficController {
    pub async fn spawn(policy_config: PolicyConfig) -> Self {
        let (tx, rx) = mpsc::channel(policy_config.channel_capacity);
        let ret = Self {
            tally_channel: tx,
            blocklists: Blocklists {
                connection_ips: Arc::new(DashMap::new()),
                proxy_ips: Arc::new(DashMap::new()),
            },
        };
        let blocklists = ret.blocklists.clone();
        spawn_monitored_task!(run_tally_loop(rx, policy_config, blocklists));
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

    pub async fn check(
        &self,
        connection_ip: Option<SocketAddr>,
        proxy_ip: Option<SocketAddr>,
    ) -> bool {
        let connection_check =
            self.check_and_clear_blocklist(connection_ip, self.blocklists.connection_ips.clone());
        let proxy_check =
            self.check_and_clear_blocklist(proxy_ip, self.blocklists.proxy_ips.clone());
        let (conn_check, proxy_check) = futures::future::join(connection_check, proxy_check).await;
        conn_check && proxy_check
    }

    async fn check_and_clear_blocklist(
        &self,
        ip: Option<SocketAddr>,
        blocklist: BlocklistT,
    ) -> bool {
        let ip = match ip {
            Some(ip) => ip,
            None => return true,
        };
        let now = SystemTime::now();
        match blocklist.get(&ip) {
            Some(expiration) if now >= *expiration => {
                blocklist.remove(&ip);
                true
            }
            None => true,
            _ => false,
        }
    }
}

async fn run_tally_loop(
    mut receiver: mpsc::Receiver<TrafficTally>,
    policy_config: PolicyConfig,
    blocklists: Blocklists,
) {
    let mut spam_policy = policy_config.clone().to_spam_policy();
    let mut error_policy = policy_config.clone().to_error_policy();
    let spam_blocklists = Arc::new(blocklists.clone());
    let error_blocklists = Arc::new(blocklists);
    loop {
        tokio::select! {
            received = receiver.recv() => match received {
                Some(tally) => {
                    handle_spam_tally(&mut spam_policy, &policy_config, tally.clone(), spam_blocklists.clone()).await;
                    handle_error_tally(&mut error_policy, &policy_config, tally, error_blocklists.clone()).await;
                }
                None => {
                    info!("TrafficController tally channel closed by all senders");
                    return;
                },
            }
        }
    }
}

async fn handle_error_tally(
    policy: &mut TrafficControlPolicy,
    config: &PolicyConfig,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
) {
    let err = if let Some(err) = tally.clone().result.err() {
        err
    } else {
        return;
    };
    if config.tallyable_error_codes.contains(&err) {
        handle_tally_impl(policy, config, tally, blocklists).await
    }
}

async fn handle_spam_tally(
    policy: &mut TrafficControlPolicy,
    config: &PolicyConfig,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
) {
    handle_tally_impl(policy, config, tally, blocklists).await
}

async fn handle_tally_impl(
    policy: &mut TrafficControlPolicy,
    config: &PolicyConfig,
    tally: TrafficTally,
    blocklists: Arc<Blocklists>,
) {
    let PolicyResponse {
        block_connection_ip,
        block_proxy_ip,
    } = policy.handle_tally(tally.clone());
    if let Some(ip) = block_connection_ip {
        blocklists.connection_ips.insert(
            ip,
            SystemTime::now() + Duration::from_secs(config.connection_blocklist_ttl_sec),
        );
    }
    if let Some(ip) = block_proxy_ip {
        blocklists.proxy_ips.insert(
            ip,
            SystemTime::now() + Duration::from_secs(config.proxy_blocklist_ttl_sec),
        );
    }
}
