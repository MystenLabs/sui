// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use mysten_metrics::spawn_monitored_task;
use parking_lot::RwLock;
use sui_types::traffic_control::{
    Policy, PolicyConfig, PolicyResponse, TrafficControlPolicy, TrafficTally,
};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing::warn;

type BlocklistT = Arc<RwLock<HashMap<SocketAddr, DateTime<Utc>>>>;

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
                connection_ips: Arc::new(RwLock::new(HashMap::new())),
                proxy_ips: Arc::new(RwLock::new(HashMap::new())),
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
        match (connection_ip, proxy_ip) {
            (Some(connection_ip), _) => self.check_and_clear_blocklist(connection_ip, true).await,
            (_, Some(proxy_ip)) => self.check_and_clear_blocklist(proxy_ip, false).await,
            _ => true,
        }
    }

    async fn check_and_clear_blocklist(&self, ip: SocketAddr, connection_ips: bool) -> bool {
        let blocklist = if connection_ips {
            self.blocklists.connection_ips.clone()
        } else {
            self.blocklists.proxy_ips.clone()
        };

        let now = Utc::now();
        let expiration = blocklist.read().get(&ip).copied();
        match expiration {
            Some(expiration) if now >= expiration => {
                blocklist.write().remove(&ip);
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
                    panic!("TrafficController tally channel closed unexpectedly");
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
    // TODO -- add update of spam blocklist
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
    if block_connection_ip {
        blocklists.connection_ips.write().insert(
            tally
                .connection_ip
                .expect("Expected connection IP if policy is blocking it"),
            Utc::now()
                + chrono::Duration::seconds(
                    config.connection_blocklist_ttl_sec.try_into().unwrap(),
                ),
        );
    }
    if block_proxy_ip {
        blocklists.proxy_ips.write().insert(
            tally
                .proxy_ip
                .expect("Expected proxy IP if policy is blocking it"),
            Utc::now()
                + chrono::Duration::seconds(config.proxy_blocklist_ttl_sec.try_into().unwrap()),
        );
    }
}
