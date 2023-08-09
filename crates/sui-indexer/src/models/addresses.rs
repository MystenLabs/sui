// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel::QueryableByName;

use sui_json_rpc_types::AddressMetrics;

use crate::schema::{active_addresses, address_stats, addresses};
use crate::types::AddressData;

#[derive(Queryable, Insertable, Clone, Debug)]
#[diesel(table_name = addresses, primary_key(account_address))]
pub struct Address {
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: i64,
    pub last_appearance_tx: String,
    pub last_appearance_time: i64,
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[diesel(table_name = active_addresses, primary_key(account_address))]
pub struct ActiveAddress {
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: i64,
    pub last_appearance_tx: String,
    pub last_appearance_time: i64,
}

pub fn dedup_from_and_to_addresses(addrs: Vec<AddressData>) -> Vec<Address> {
    let addr_map = addrs.into_iter().fold(HashMap::new(), |mut acc, addr| {
        let key = addr.account_address.clone();
        let value = Address {
            account_address: addr.account_address,
            first_appearance_tx: addr.transaction_digest.clone(),
            first_appearance_time: addr.timestamp_ms,
            last_appearance_tx: addr.transaction_digest,
            last_appearance_time: addr.timestamp_ms,
        };
        acc.entry(key)
            .and_modify(|v: &mut Address| {
                if v.first_appearance_time > value.first_appearance_time {
                    v.first_appearance_time = value.first_appearance_time;
                    v.first_appearance_tx = value.first_appearance_tx.clone();
                }
                if v.last_appearance_time < value.last_appearance_time {
                    v.last_appearance_time = value.last_appearance_time;
                    v.last_appearance_tx = value.last_appearance_tx.clone();
                }
            })
            .or_insert(value);
        acc
    });
    addr_map.into_values().collect()
}

pub fn dedup_from_addresses(from_addrs: Vec<AddressData>) -> Vec<ActiveAddress> {
    let active_addr_map = from_addrs
        .into_iter()
        .fold(HashMap::new(), |mut acc, addr| {
            let key = addr.account_address.clone();
            let value = ActiveAddress {
                account_address: addr.account_address,
                first_appearance_tx: addr.transaction_digest.clone(),
                first_appearance_time: addr.timestamp_ms,
                last_appearance_tx: addr.transaction_digest,
                last_appearance_time: addr.timestamp_ms,
            };
            acc.entry(key)
                .and_modify(|v: &mut ActiveAddress| {
                    if v.first_appearance_time > value.first_appearance_time {
                        v.first_appearance_time = value.first_appearance_time;
                        v.first_appearance_tx = value.first_appearance_tx.clone();
                    }
                    if v.last_appearance_time < value.last_appearance_time {
                        v.last_appearance_time = value.last_appearance_time;
                        v.last_appearance_tx = value.last_appearance_tx.clone();
                    }
                })
                .or_insert(value);
            acc
        });
    active_addr_map.into_values().collect()
}

#[derive(Queryable, Insertable, Clone, Debug)]
#[diesel(table_name = address_stats, primary_key(checkpoint))]
pub struct AddressStats {
    pub checkpoint: i64,
    pub epoch: i64,
    pub timestamp_ms: i64,
    pub cumulative_addresses: i64,
    pub cumulative_active_addresses: i64,
    pub daily_active_addresses: i64,
}

impl From<AddressStats> for AddressMetrics {
    fn from(stats: AddressStats) -> Self {
        AddressMetrics {
            checkpoint: stats.checkpoint as u64,
            epoch: stats.epoch as u64,
            timestamp_ms: stats.timestamp_ms as u64,
            cumulative_addresses: stats.cumulative_addresses as u64,
            cumulative_active_addresses: stats.cumulative_active_addresses as u64,
            daily_active_addresses: stats.daily_active_addresses as u64,
        }
    }
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct DBAddressStats {
    #[diesel(sql_type = BigInt)]
    pub checkpoint: i64,
    #[diesel(sql_type = BigInt)]
    pub epoch: i64,
    #[diesel(sql_type = BigInt)]
    pub timestamp_ms: i64,
    #[diesel(sql_type = BigInt)]
    pub cumulative_addresses: i64,
    #[diesel(sql_type = BigInt)]
    pub cumulative_active_addresses: i64,
    #[diesel(sql_type = BigInt)]
    pub daily_active_addresses: i64,
}

impl From<DBAddressStats> for AddressStats {
    fn from(stats: DBAddressStats) -> Self {
        AddressStats {
            checkpoint: stats.checkpoint,
            epoch: stats.epoch,
            timestamp_ms: stats.timestamp_ms,
            cumulative_addresses: stats.cumulative_addresses,
            cumulative_active_addresses: stats.cumulative_active_addresses,
            daily_active_addresses: stats.daily_active_addresses,
        }
    }
}
