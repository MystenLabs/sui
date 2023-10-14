// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use diesel::prelude::*;

use crate::schema_v2::{active_addresses, address_metrics, addresses};

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = addresses)]
pub struct StoredAddress {
    pub address: Vec<u8>,
    pub first_appearance_tx: i64,
    pub first_appearance_time: i64,
    pub last_appearance_tx: i64,
    pub last_appearance_time: i64,
}

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = active_addresses)]
pub struct StoredActiveAddress {
    pub address: Vec<u8>,
    pub first_appearance_tx: i64,
    pub first_appearance_time: i64,
    pub last_appearance_tx: i64,
    pub last_appearance_time: i64,
}

impl From<StoredAddress> for StoredActiveAddress {
    fn from(address: StoredAddress) -> Self {
        StoredActiveAddress {
            address: address.address,
            first_appearance_tx: address.first_appearance_tx,
            first_appearance_time: address.first_appearance_time,
            last_appearance_tx: address.last_appearance_tx,
            last_appearance_time: address.last_appearance_time,
        }
    }
}

#[derive(Clone, Debug, Default, Queryable, Insertable)]
#[diesel(table_name = address_metrics)]
pub struct StoredAddressMetrics {
    pub checkpoint: i64,
    pub epoch: i64,
    pub timestamp_ms: i64,
    pub cumulative_addresses: i64,
    pub cumulative_active_addresses: i64,
    pub daily_active_addresses: i64,
}

#[derive(Clone, Debug)]
pub struct AddressInfoToCommit {
    pub address: Vec<u8>,
    pub tx_seq: i64,
    pub timestamp_ms: i64,
}

pub fn dedup_addresses(addrs_to_commit: Vec<AddressInfoToCommit>) -> Vec<StoredAddress> {
    let mut compressed_addr_map: HashMap<_, StoredAddress> = HashMap::new();
    for addr_to_commit in addrs_to_commit {
        let entry = compressed_addr_map
            .entry(addr_to_commit.address.clone())
            .or_insert_with(|| StoredAddress {
                address: addr_to_commit.address.clone(),
                first_appearance_time: addr_to_commit.timestamp_ms,
                first_appearance_tx: addr_to_commit.tx_seq,
                last_appearance_time: addr_to_commit.timestamp_ms,
                last_appearance_tx: addr_to_commit.tx_seq,
            });

        if addr_to_commit.timestamp_ms < entry.first_appearance_time {
            entry.first_appearance_time = addr_to_commit.timestamp_ms;
            entry.first_appearance_tx = addr_to_commit.tx_seq;
        }
        if addr_to_commit.timestamp_ms > entry.last_appearance_time {
            entry.last_appearance_time = addr_to_commit.timestamp_ms;
            entry.last_appearance_tx = addr_to_commit.tx_seq;
        }
    }
    compressed_addr_map.values().cloned().collect()
}
