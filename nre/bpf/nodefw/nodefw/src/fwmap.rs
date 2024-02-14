// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::time::get_ktime_get_ns;
use anyhow::Error;
use aya::maps::MapData;
use aya::{maps::HashMap, Bpf};
use log::{error, info};
use nodefw_common::Rule;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::net::{IpAddr, Ipv6Addr};
use std::str::FromStr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct Firewall {
    inner: HashMap<MapData, [u8; 16usize], Rule>,
    expirations: BinaryHeap<TtlRecord>,
}
impl Firewall {
    pub fn new(map_name: &str, bpf: &mut Bpf) -> Self {
        Self {
            inner: HashMap::try_from(bpf.take_map(map_name).unwrap()).unwrap(),
            expirations: BinaryHeap::new(),
        }
    }
    pub fn add(&mut self, ip: &str, rule: Rule) -> Result<(), Error> {
        let parsed_ip = match IpAddr::from_str(ip) {
            Ok(IpAddr::V4(v)) => v.to_ipv6_compatible(),
            Ok(IpAddr::V6(v)) => v,
            Err(e) => {
                error!("{}", e);
                return Err(e.into());
            }
        };
        self.inner.insert(parsed_ip.octets(), rule, 0)?;
        if rule.ttl > 0 {
            self.expirations.push(TtlRecord {
                key: parsed_ip.octets(),
                expiration_time: rule.ttl,
            });
        }
        Ok(())
    }
    fn remove_expired_keys(&mut self) {
        let now = get_ktime_get_ns();
        while let Some(expiry) = self.expirations.pop() {
            if expiry.expiration_time <= now {
                if let Err(e) = self.inner.remove(&expiry.key) {
                    error!("unable to remove key {}", e);
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
                self.expirations.push(expiry);
                break;
            }
        }
    }
}

pub async fn ttl_watcher(ctx: CancellationToken, ttls: RefCell<Firewall>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Remove expired keys
                ttls.borrow_mut().remove_expired_keys();
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
