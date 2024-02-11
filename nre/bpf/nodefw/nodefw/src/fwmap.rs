// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::server::BlockAddress;
use crate::time::{get_ktime_get_ns, ttl};
use anyhow::Result;
use aya::maps::MapData;
use aya::util::nr_cpus;
use aya::{
    maps::{PerCpuHashMap, PerCpuValues},
    Bpf,
};
use log::{error, info, warn};
use nodefw_common::Rule;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::net::{IpAddr, Ipv6Addr};
use std::ops::Deref;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct Firewall {
    inner: Arc<RwLock<PerCpuHashMap<MapData, [u8; 16usize], Rule>>>,
    expirations: Arc<RwLock<BinaryHeap<TtlRecord>>>,
}
impl Firewall {
    pub fn new(map_name: &str, bpf: &mut Bpf) -> Self {
        Self {
            inner: Arc::new(RwLock::new(
                PerCpuHashMap::try_from(bpf.take_map(map_name).unwrap()).unwrap(),
            )),
            expirations: Arc::new(RwLock::new(BinaryHeap::new())),
        }
    }
    pub fn block_addresses(&mut self, addresses: Vec<BlockAddress>) -> Result<()> {
        let inner_guard = self.inner.clone();
        let mut fw = inner_guard.write().unwrap();
        let expirations_guard = self.expirations.clone();
        let mut expirations = expirations_guard.write().unwrap();
        for addr in addresses {
            let parsed_ip = match IpAddr::from_str(&addr.source_address) {
                Ok(IpAddr::V4(v)) => v.to_ipv6_compatible(),
                Ok(IpAddr::V6(v)) => v,
                Err(e) => {
                    error!("{}", e);
                    return Err(e.into());
                }
            };
            let rule = Rule {
                port: addr.destination_port,
                ttl: ttl(Duration::from_secs(addr.ttl)),
            };
            let values = PerCpuValues::try_from(vec![rule; nr_cpus()?])?;
            fw.insert(parsed_ip.octets(), values, 0)?;
            if addr.ttl > 0 {
                expirations.push(TtlRecord {
                    key: parsed_ip.octets(),
                    expiration_time: rule.ttl,
                });
            }
        }
        Ok(())
    }
    pub fn list_addresses(&self) -> Result<Vec<BlockAddress>> {
        let inner_guard = self.inner.clone();
        let fw = inner_guard.write().unwrap();
        Ok(fw
            .iter()
            // aya PerCpuHashMap uses a Result<(k,v), Error>
            .filter_map(|map_iter| match map_iter {
                Ok((k, v)) => Some((k, v)),
                Err(_) => None,
            })
            .map(|(k, v): ([u8; 16usize], PerCpuValues<Rule>)| {
                let Some(rule) = v.deref().clone().into_vec().pop() else {
                    panic!("invariant violation - we should always have values here");
                };
                let v6_source_address = Ipv6Addr::from(k);
                let source_address = match v6_source_address.to_ipv4() {
                    Some(v) => v.to_string(),
                    None => v6_source_address.to_string(),
                };
                let destination_port = rule.port;
                let ttl = rule.ttl;
                BlockAddress {
                    source_address,
                    destination_port,
                    ttl,
                }
            })
            .collect())
    }
    fn remove_expired_keys(&self) {
        let inner_guard = self.inner.clone();
        let mut fw = inner_guard.write().unwrap();
        let expirations_guard = self.expirations.clone();
        let mut expirations = expirations_guard.write().unwrap();
        let now = get_ktime_get_ns();
        while let Some(expiry) = expirations.pop() {
            if expiry.expiration_time <= now {
                if let Err(e) = fw.remove(&expiry.key) {
                    // not always an actual error - there is some racey behavior if you
                    // add or remove in a certain combination.  the kernel guarantees safe
                    // access and sets but sometimes a get or a remove will fail until
                    // thing settle.
                    warn!("unable to remove key {}", e);
                    break;
                };
                let removed = Ipv6Addr::from(expiry.key);
                // try converting to a v4 address, else print what we have
                if let Some(v4) = removed.to_ipv4() {
                    info!("expired {:}", v4);
                } else {
                    info!("expired {:}", removed);
                }
            } else {
                // Re-insert the record back into the heap if not expired
                expirations.push(expiry);
                break;
            }
        }
    }
}

pub async fn ttl_watcher(ctx: CancellationToken, fw: Arc<RwLock<Firewall>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let fw_guard = fw.clone();
    info!("ttl_watcher is enabled");
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Remove expired keys
                let fw = fw_guard.write().unwrap();
                fw.remove_expired_keys();
                drop(fw);
            }
            _ = ctx.cancelled() => {
                info!("ttl_watcher received cancellation request");
                return;
            }
        }
    }
}

#[derive(Eq, PartialEq)]
struct TtlRecord {
    key: [u8; 16usize],
    expiration_time: u64,
}

impl Ord for TtlRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        other.expiration_time.cmp(&self.expiration_time)
    }
}

impl PartialOrd for TtlRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
