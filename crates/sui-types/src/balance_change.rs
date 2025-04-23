// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::coin::Coin;
use crate::effects::TransactionEffects;
use crate::object::Object;
use crate::object::Owner;
use move_core_types::language_storage::TypeTag;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct BalanceChange {
    /// Owner of the balance change
    pub address: SuiAddress,

    /// Type of the Coin
    pub coin_type: TypeTag,

    /// The amount indicate the balance value changes.
    ///
    /// A negative amount means spending coin value and positive means receiving coin value.
    pub amount: i128,
}

fn coins(objects: &[Object]) -> impl Iterator<Item = (&SuiAddress, TypeTag, u64)> + '_ {
    objects.iter().filter_map(|object| {
        let address = match object.owner() {
            Owner::AddressOwner(sui_address) | Owner::ObjectOwner(sui_address) => sui_address,
            Owner::Shared { .. } | Owner::Immutable => return None,
            Owner::ConsensusV2 { .. } => todo!(),
        };
        let (coin_type, balance) = Coin::extract_balance_if_coin(object).ok().flatten()?;
        Some((address, coin_type, balance))
    })
}

pub fn derive_balance_changes(
    _effects: &TransactionEffects,
    input_objects: &[Object],
    output_objects: &[Object],
) -> Vec<BalanceChange> {
    // 1. subtract all input coins
    let balances = coins(input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin_type, balance)| {
            *acc.entry((address, coin_type)).or_default() -= balance as i128;
            acc
        },
    );

    // 2. add all mutated/output coins
    let balances =
        coins(output_objects).fold(balances, |mut acc, (address, coin_type, balance)| {
            *acc.entry((address, coin_type)).or_default() += balance as i128;
            acc
        });

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address: *address,
                coin_type,
                amount,
            })
        })
        .collect()
}
