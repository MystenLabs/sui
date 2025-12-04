// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::debug_fatal;
use sui_types::{
    TypeTag, accumulator_metadata::AccumulatorOwner, accumulator_root::AccumulatorValue,
    balance::Balance, base_types::SuiAddress, error::SuiResult, storage::ChildObjectResolver,
};

use crate::jsonrpc_index::IndexStoreTables;

/// Get the balance for a given owner address (which can be a wallet or an object)
/// and currency type (e.g. 0x2::sui::SUI)
pub fn get_balance(
    owner: SuiAddress,
    child_object_resolver: &dyn ChildObjectResolver,
    currency_type: TypeTag,
) -> SuiResult<u64> {
    let balance_type = Balance::type_tag(currency_type);
    let address_balance =
        AccumulatorValue::load(child_object_resolver, None, owner, &balance_type)?
            .and_then(|b| b.as_u128())
            .unwrap_or(0);

    let u64_balance = if address_balance > u64::MAX as u128 {
        // This will not happen with normal currency types which have a max supply of u64::MAX
        // But you can create "fake" supplies (with no metadata or treasury cap) and overlow
        // the u64 limit.
        tracing::warn!(
            "address balance for {} {} is greater than u64::MAX",
            owner,
            balance_type.to_canonical_string(true)
        );
        u64::MAX
    } else {
        address_balance as u64
    };

    Ok(u64_balance)
}

/// Get all currency types for a given owner address (which can be a wallet or an object)
pub fn get_currency_types_for_owner(
    owner: SuiAddress,
    child_object_resolver: &dyn ChildObjectResolver,
    index_tables: &IndexStoreTables,
) -> SuiResult<Vec<TypeTag>> {
    let Some(owner_obj) = AccumulatorOwner::load_object(child_object_resolver, None, owner)? else {
        return Ok(Vec::new());
    };

    let owner_version = owner_obj.version();

    let accumulator_owner_obj = AccumulatorOwner::from_object(owner_obj)?;

    if accumulator_owner_obj.owner != owner {
        debug_fatal!("owner object owner does not match the requested owner");
        return Ok(Vec::new());
    };

    let bag_id = accumulator_owner_obj.balances.id.object_id();

    // get all balance types for the owner
    let accumulator_metadata: Vec<_> = index_tables
        .get_dynamic_fields_iterator(*bag_id, None)?
        .collect();

    let mut currency_types = Vec::new();
    for result in accumulator_metadata {
        let (object_id, _) = result?;

        if let Some(object) =
            child_object_resolver.read_child_object(bag_id, &object_id, owner_version)?
        {
            let ty = object
                .data
                .try_as_move()
                .expect("accumulator metadata object is not a move object")
                .type_();

            let Some(currency_type) = ty.balance_accumulator_metadata_field_type_maybe() else {
                // This should currently never happen. But in the future, there may be non-balance
                // accumulator types, in which case we would need to skip them here.
                debug_fatal!(
                    "accumulator metadata object is not a balance accumulator metadata field"
                );
                continue;
            };

            currency_types.push(currency_type);
        }
    }

    Ok(currency_types)
}

/// Get all balances and corresponding currency types for a given owner address
/// (which can be a wallet or an object)
pub fn get_all_balances_for_owner(
    owner: SuiAddress,
    child_object_resolver: &dyn ChildObjectResolver,
    index_tables: &IndexStoreTables,
) -> SuiResult<Vec<(TypeTag, u64)>> {
    let currency_types = get_currency_types_for_owner(owner, child_object_resolver, index_tables)?;
    let mut balances = Vec::new();
    for currency_type in currency_types {
        let balance = get_balance(owner, child_object_resolver, currency_type.clone())?;
        balances.push((currency_type, balance));
    }
    Ok(balances)
}
