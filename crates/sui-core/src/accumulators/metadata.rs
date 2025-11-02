// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::debug_fatal;
use sui_types::{
    TypeTag, accumulator_metadata::AccumulatorOwner, base_types::SuiAddress, error::SuiResult,
    storage::ChildObjectResolver,
};

use crate::jsonrpc_index::IndexStoreTables;

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
                dbg!(&ty);
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
