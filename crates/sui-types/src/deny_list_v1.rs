// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SUI_DENY_LIST_OBJECT_ID;
use crate::base_types::{SequenceNumber, SuiAddress};
use crate::collection_types::{Bag, Table, VecSet};
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::{UserInputError, UserInputResult};
use crate::id::{ID, UID};
use crate::object::{Object, Owner};
use crate::storage::ObjectStore;
use crate::transaction::{CheckedInputObjects, ReceivingObjects};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tracing::debug;
use tracing::error;

pub const DENY_LIST_MODULE: &IdentStr = ident_str!("deny_list");
pub const DENY_LIST_CREATE_FUNC: &IdentStr = ident_str!("create");

pub const DENY_LIST_COIN_TYPE_INDEX: u64 = 0;

/// Rust representation of the Move type 0x2::deny_list::DenyList.
/// It has a bag that contains the deny lists for different system types.
/// At creation, there is only one type (at key 0), which is the Coin type.
/// We also take advantage of the dynamic nature of Bag to add more types in the future,
/// as well as making changes to the deny lists for existing types.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DenyList {
    pub id: UID,
    pub lists: Bag,
}

/// Rust representation of the Move type 0x2::deny_list::PerTypeDenyList.
/// denied_count is a table that stores the number of denied addresses for each coin template type.
/// It can be used as a quick check to see if an address is denied for any coin template type.
/// denied_addresses is a table that stores all the addresses that are denied for each coin template type.
/// The key to the table is the coin template type in string form: package_id::module_name::struct_name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerTypeDenyList {
    pub id: UID,
    // Table<address, u64>
    pub denied_count: Table,
    // Table<vec<u8>, VecSet<address>>
    pub denied_addresses: Table,
}

/// Checks coin denylist v1 at signing time.
/// It checks that none of the coin types in the transaction are denied for an address they are
/// debited from. This includes the sender, the sponsor, and any allowance's funding source.
pub fn check_coin_deny_list_v1(
    sender: SuiAddress,
    input_objects: &CheckedInputObjects,
    receiving_objects: &ReceivingObjects,
    funds_withdrawals: BTreeSet<(SuiAddress, String)>,
    object_store: &dyn ObjectStore,
) -> UserInputResult {
    let addresses_by_coin_type = addresses_by_coin_type_for_denylist_check(
        sender,
        input_objects,
        receiving_objects,
        funds_withdrawals,
    );

    let Some(deny_list) = get_coin_deny_list(object_store) else {
        // TODO: This is where we should fire an invariant violation metric.
        if cfg!(debug_assertions) {
            panic!("Failed to get the coin deny list");
        } else {
            return Ok(());
        }
    };
    check_deny_list_v1_impl(deny_list, addresses_by_coin_type, object_store)
}

/// Every address here costs a deny-list lookup at signing. Today the set is the sender plus one
/// funder per withdrawal, at most 10 withdrawals. Raising that limit must revisit this bound.
const MAX_DENY_LIST_ADDRESSES: usize = 11;

/// Generates a map of (coin_type -> addresses) to be checked for deny-list.
/// Includes the sender's input objects, and the withdrawals from address balances (which may
/// have funding sources other than the sender).
pub(crate) fn addresses_by_coin_type_for_denylist_check(
    sender: SuiAddress,
    input_objects: &CheckedInputObjects,
    receiving_objects: &ReceivingObjects,
    funds_withdrawals: BTreeSet<(SuiAddress, String)>,
) -> BTreeMap<String, BTreeSet<SuiAddress>> {
    let mut addresses_by_coin_type: BTreeMap<String, BTreeSet<SuiAddress>> = BTreeMap::new();
    for coin_type in input_object_coin_types_for_denylist_check(input_objects, receiving_objects) {
        addresses_by_coin_type
            .entry(coin_type)
            .or_default()
            .insert(sender);
    }
    for (funder, coin_type) in funds_withdrawals {
        // The funder's balance is debited and the sender directs the spend, so check both.
        let addresses = addresses_by_coin_type.entry(coin_type).or_default();
        addresses.insert(sender);
        addresses.insert(funder);
    }
    debug_assert!(
        addresses_by_coin_type.values().all(|a| !a.is_empty()),
        "every set contains at least the sender"
    );
    // If this triggers, it means the max withdrawals limit was changed
    // and we need to decide if we want to add a non-debug limit for deny-list owner checks.
    debug_assert!(
        addresses_by_coin_type
            .values()
            .flatten()
            .collect::<BTreeSet<_>>()
            .len()
            <= MAX_DENY_LIST_ADDRESSES,
        "deny-list address set exceeds its bound"
    );
    addresses_by_coin_type
}

/// Returns all unique coin types in canonical string form from the input objects and receiving objects.
/// It filters out SUI coins since it's known that it's not a regulated coin.
pub(crate) fn input_object_coin_types_for_denylist_check(
    input_objects: &CheckedInputObjects,
    receiving_objects: &ReceivingObjects,
) -> BTreeSet<String> {
    let all_objects = input_objects
        .inner()
        .iter_objects()
        .chain(receiving_objects.iter_objects());
    all_objects
        .filter_map(|obj| {
            if obj.is_gas_coin() {
                None
            } else {
                obj.coin_type_maybe()
                    .map(|type_tag| type_tag.to_canonical_string(false))
            }
        })
        .collect()
}

fn check_deny_list_v1_impl(
    deny_list: PerTypeDenyList,
    addresses_by_coin_type: BTreeMap<String, BTreeSet<SuiAddress>>,
    object_store: &dyn ObjectStore,
) -> UserInputResult {
    let addresses: BTreeSet<SuiAddress> =
        addresses_by_coin_type.values().flatten().copied().collect();

    let denied_anywhere: BTreeSet<SuiAddress> = addresses
        .into_iter()
        .filter(|address| denied_count(&deny_list, *address, object_store) > 0)
        .collect();
    if denied_anywhere.is_empty() {
        return Ok(());
    }
    for (coin_type, addresses) in addresses_by_coin_type {
        if addresses.is_disjoint(&denied_anywhere) {
            continue;
        }
        let Ok(denied_addresses) = get_dynamic_field_from_store::<Vec<u8>, VecSet<SuiAddress>>(
            object_store,
            deny_list.denied_addresses.id,
            &coin_type.clone().into_bytes(),
        ) else {
            continue;
        };
        let denied_addresses: BTreeSet<_> = denied_addresses.contents.into_iter().collect();
        if let Some(&address) = addresses.intersection(&denied_addresses).next() {
            debug!(
                "Address {} is denied for coin package {:?}",
                address, coin_type
            );
            return Err(UserInputError::AddressDeniedForCoin { address, coin_type });
        }
    }
    Ok(())
}

/// How many coin types `address` is denied for; no entry means none.
fn denied_count(
    deny_list: &PerTypeDenyList,
    address: SuiAddress,
    object_store: &dyn ObjectStore,
) -> u64 {
    get_dynamic_field_from_store(object_store, deny_list.denied_count.id, &address).unwrap_or(0)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoinDenyCap {
    pub id: UID,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegulatedCoinMetadata {
    pub id: UID,
    pub coin_metadata_object: ID,
    pub deny_cap_object: ID,
}

pub fn get_deny_list_root_object(object_store: &dyn ObjectStore) -> Option<Object> {
    // TODO: We should return error if this is not found.
    match object_store.get_object(&SUI_DENY_LIST_OBJECT_ID) {
        Some(obj) => Some(obj),
        None => {
            error!("Deny list object not found");
            None
        }
    }
}

pub fn get_coin_deny_list(object_store: &dyn ObjectStore) -> Option<PerTypeDenyList> {
    get_deny_list_root_object(object_store).and_then(|obj| {
        let deny_list: DenyList = obj
            .to_rust()
            .expect("DenyList object type must be consistent");
        match get_dynamic_field_from_store(
            object_store,
            *deny_list.lists.id.object_id(),
            &DENY_LIST_COIN_TYPE_INDEX,
        ) {
            Ok(deny_list) => Some(deny_list),
            Err(err) => {
                error!("Failed to get deny list inner state: {}", err);
                None
            }
        }
    })
}

pub fn get_deny_list_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> Option<SequenceNumber> {
    get_deny_list_root_object(object_store).map(|obj| match obj.owner {
        Owner::Shared {
            initial_shared_version,
        } => initial_shared_version,
        _ => unreachable!("Deny list object must be shared"),
    })
}
