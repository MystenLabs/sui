// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{de::DeserializeOwned, Serialize};
use std::{path::PathBuf, str::FromStr};
use sui_types::digests::TransactionDigest;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;

use crate::get_db_entries;
use move_core_types::language_storage::ModuleId;
use std::fmt::Debug;
use sui_core::jsonrpc_index::IndexStoreTables;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TxSequenceNumber},
    Identifier, TypeTag,
};

#[derive(Clone, Debug)]
pub enum SearchRange<T: Serialize + Clone + Debug> {
    ExclusiveLastKey(T),
    Count(u64),
}

impl<T: Serialize + Clone + Debug + FromStr> FromStr for SearchRange<T>
where
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let last_key = T::from_str(s).map_err(|e| anyhow!("Failed to parse last_key: {:?}", e))?;
        Ok(SearchRange::ExclusiveLastKey(last_key))
    }
}

/// Until we use a proc macro to auto derive this, we have to make sure to update the
/// `search_index` function below when adding new tables.
pub fn search_index(
    db_path: PathBuf,
    table_name: String,
    start: String,
    termination: SearchRange<String>,
) -> Result<Vec<(String, String)>, anyhow::Error> {
    let start = start.as_str();
    println!("Opening db at {:?} ...", db_path);
    let db_read_only_handle =
        IndexStoreTables::get_read_only_handle(db_path, None, None, MetricConf::default());
    match table_name.as_str() {
        "transactions_from_addr" => {
            get_db_entries!(
                db_read_only_handle.transactions_from_addr,
                from_addr_seq,
                start,
                termination
            )
        }
        "transactions_to_addr" => {
            get_db_entries!(
                db_read_only_handle.transactions_to_addr,
                from_addr_seq,
                start,
                termination
            )
        }
        "transactions_by_input_object_id" => {
            get_db_entries!(
                db_read_only_handle.transactions_by_input_object_id,
                from_id_seq,
                start,
                termination
            )
        }
        "transactions_by_mutated_object_id" => {
            get_db_entries!(
                db_read_only_handle.transactions_by_mutated_object_id,
                from_id_seq,
                start,
                termination
            )
        }
        "transactions_by_move_function" => {
            get_db_entries!(
                db_read_only_handle.transactions_by_move_function,
                from_id_module_function_txseq,
                start,
                termination
            )
        }
        "transaction_order" => {
            get_db_entries!(
                db_read_only_handle.transaction_order,
                u64::from_str,
                start,
                termination
            )
        }
        "transactions_seq" => {
            get_db_entries!(
                db_read_only_handle.transactions_seq,
                TransactionDigest::from_str,
                start,
                termination
            )
        }
        "owner_index" => {
            get_db_entries!(
                db_read_only_handle.owner_index,
                from_addr_oid,
                start,
                termination
            )
        }
        "coin_index" => {
            get_db_entries!(
                db_read_only_handle.coin_index,
                from_addr_str_oid,
                start,
                termination
            )
        }
        "dynamic_field_index" => {
            get_db_entries!(
                db_read_only_handle.dynamic_field_index,
                from_oid_oid,
                start,
                termination
            )
        }
        "event_by_event_module" => {
            get_db_entries!(
                db_read_only_handle.event_by_event_module,
                from_module_id_and_event_id,
                start,
                termination
            )
        }
        "event_by_move_module" => {
            get_db_entries!(
                db_read_only_handle.event_by_move_module,
                from_module_id_and_event_id,
                start,
                termination
            )
        }
        "event_order" => {
            get_db_entries!(
                db_read_only_handle.event_order,
                from_event_id,
                start,
                termination
            )
        }
        "event_by_sender" => {
            get_db_entries!(
                db_read_only_handle.event_by_sender,
                from_address_and_event_id,
                start,
                termination
            )
        }
        _ => Err(anyhow!("Invalid or unsupported table: {}", table_name)),
    }
}

#[macro_export]
macro_rules! get_db_entries {
    ($db_map:expr, $key_converter:expr, $start:expr, $term:expr) => {{
        let key = $key_converter($start)?;
        println!("Searching from key: {:?}", key);
        let termination = match $term {
            SearchRange::ExclusiveLastKey(last_key) => {
                println!(
                    "Retrieving all keys up to (but not including) key: {:?}",
                    key
                );
                SearchRange::ExclusiveLastKey($key_converter(last_key.as_str())?)
            }
            SearchRange::Count(count) => {
                println!("Retrieving up to {} keys", count);
                SearchRange::Count(count)
            }
        };

        $db_map.try_catch_up_with_primary().unwrap();
        get_entries_to_str(&$db_map, key, termination)
    }};
}

fn get_entries_to_str<K, V>(
    db_map: &DBMap<K, V>,
    start: K,
    termination: SearchRange<K>,
) -> Result<Vec<(String, String)>, anyhow::Error>
where
    K: Serialize + serde::de::DeserializeOwned + Clone + Debug,
    V: serde::Serialize + DeserializeOwned + Clone + Debug,
{
    get_entries(db_map, start, termination).map(|entries| {
        entries
            .into_iter()
            .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
            .collect()
    })
}

fn get_entries<K, V>(
    db_map: &DBMap<K, V>,
    start: K,
    termination: SearchRange<K>,
) -> Result<Vec<(K, V)>, anyhow::Error>
where
    K: Serialize + serde::de::DeserializeOwned + Clone + std::fmt::Debug,
    V: serde::Serialize + DeserializeOwned + Clone,
{
    let mut entries = Vec::new();
    match termination {
        SearchRange::ExclusiveLastKey(exclusive_last_key) => {
            let iter = db_map.safe_iter_with_bounds(Some(start), Some(exclusive_last_key));

            for result in iter {
                let (key, value) = result?;
                entries.push((key.clone(), value.clone()));
            }
        }
        SearchRange::Count(mut count) => {
            let mut iter = db_map.safe_iter_with_bounds(Some(start), None);

            while count > 0 {
                if let Some(result) = iter.next() {
                    let (key, value) = result?;
                    entries.push((key.clone(), value.clone()));
                } else {
                    break;
                }
                count -= 1;
            }
        }
    }
    Ok(entries)
}

fn from_addr_seq(s: &str) -> Result<(SuiAddress, TxSequenceNumber), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid address, sequence number pair"));
    }
    let address = SuiAddress::from_str(tokens[0].trim())?;
    let sequence_number = TxSequenceNumber::from_str(tokens[1].trim())?;

    Ok((address, sequence_number))
}

fn from_id_seq(s: &str) -> Result<(ObjectID, TxSequenceNumber), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid object id, sequence number pair"));
    }
    let oid = ObjectID::from_str(tokens[0].trim())?;
    let sequence_number = TxSequenceNumber::from_str(tokens[1].trim())?;

    Ok((oid, sequence_number))
}

fn from_id_module_function_txseq(
    s: &str,
) -> Result<(ObjectID, String, String, TxSequenceNumber), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 4 {
        return Err(anyhow!(
            "Invalid object id, module name, function name, TX sequence number quad"
        ));
    }
    let pid = ObjectID::from_str(tokens[0].trim())?;
    let module: Identifier = Identifier::from_str(tokens[1].trim())?;
    let func: Identifier = Identifier::from_str(tokens[2].trim())?;
    let seq: TxSequenceNumber = TxSequenceNumber::from_str(tokens[3].trim())?;

    Ok((pid, module.to_string(), func.to_string(), seq))
}

fn from_addr_oid(s: &str) -> Result<(SuiAddress, ObjectID), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid address, object id pair"));
    }
    let addr = SuiAddress::from_str(tokens[0].trim())?;
    let oid = ObjectID::from_str(tokens[1].trim())?;

    Ok((addr, oid))
}

fn from_addr_str_oid(s: &str) -> Result<(SuiAddress, String, ObjectID), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 3 {
        return Err(anyhow!("Invalid addr, type tag object id triplet"));
    }
    let address = SuiAddress::from_str(tokens[0].trim())?;
    let tag: TypeTag = TypeTag::from_str(tokens[1].trim())?;
    let oid: ObjectID = ObjectID::from_str(tokens[2].trim())?;

    Ok((address, tag.to_string(), oid))
}

fn from_oid_oid(s: &str) -> Result<(ObjectID, ObjectID), anyhow::Error> {
    // Remove whitespaces
    let s = s.trim();
    let tokens = s.split(',').collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid object id, object id triplet"));
    }
    let oid1 = ObjectID::from_str(tokens[0].trim())?;
    let oid2: ObjectID = ObjectID::from_str(tokens[1].trim())?;

    Ok((oid1, oid2))
}

fn from_module_id_and_event_id(
    s: &str,
) -> Result<(ModuleId, (TxSequenceNumber, usize)), anyhow::Error> {
    // Example: "0x1::Event 1234 5"
    let tokens = s.split(' ').collect::<Vec<&str>>();
    if tokens.len() != 3 {
        return Err(anyhow!("Invalid input"));
    }
    let tx_seq = TxSequenceNumber::from_str(tokens[1])?;
    let event_seq = usize::from_str(tokens[2])?;
    let tokens = tokens[0].split("::").collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid module id"));
    }
    let package = ObjectID::from_str(tokens[0].trim())?;

    Ok((
        ModuleId::new(package.into(), Identifier::from_str(tokens[1].trim())?),
        (tx_seq, event_seq),
    ))
}

fn from_event_id(s: &str) -> Result<(TxSequenceNumber, usize), anyhow::Error> {
    // Example: "1234 5"
    let tokens = s.split(' ').collect::<Vec<&str>>();
    if tokens.len() != 2 {
        return Err(anyhow!("Invalid input"));
    }
    let tx_seq = TxSequenceNumber::from_str(tokens[0])?;
    let event_seq = usize::from_str(tokens[1])?;
    Ok((tx_seq, event_seq))
}

fn from_address_and_event_id(
    s: &str,
) -> Result<(SuiAddress, (TxSequenceNumber, usize)), anyhow::Error> {
    // Example: "0x1 1234 5"
    let tokens = s.split(' ').collect::<Vec<&str>>();
    if tokens.len() != 3 {
        return Err(anyhow!("Invalid input"));
    }
    let tx_seq = TxSequenceNumber::from_str(tokens[1])?;
    let event_seq = usize::from_str(tokens[2])?;
    let address = SuiAddress::from_str(tokens[0].trim())?;
    Ok((address, (tx_seq, event_seq)))
}
