// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use aya::maps::MapData;
use aya::{maps::HashMap, Bpf};
use log::error;
use nodefw_common::Rule;
use std::net::IpAddr;
use std::str::FromStr;

pub struct Firewall {
    inner: HashMap<MapData, [u8; 16usize], Rule>,
}
impl Firewall {
    pub fn new(map_name: &str, bpf: &mut Bpf) -> Self {
        Self {
            inner: HashMap::try_from(bpf.take_map(map_name).unwrap()).unwrap(),
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
        Ok(())
    }
}
