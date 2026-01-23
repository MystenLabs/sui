// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    TypeTag, accumulator_root::AccumulatorValue, balance::Balance, base_types::SuiAddress,
    error::SuiResult, storage::ChildObjectResolver,
};

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

/// Get all balances and corresponding currency types for a given owner address
/// (which can be a wallet or an object)
pub fn get_all_balances_for_owner(
    owner: SuiAddress,
    child_object_resolver: &dyn ChildObjectResolver,
    index_store: &crate::jsonrpc_index::IndexStore,
) -> SuiResult<Vec<(TypeTag, u64)>> {
    let currency_types = index_store.get_address_balance_coin_types_iter(owner);
    let mut balances = Vec::new();
    for currency_type in currency_types {
        let balance = get_balance(owner, child_object_resolver, currency_type.clone())?;
        balances.push((currency_type, balance));
    }
    Ok(balances)
}
