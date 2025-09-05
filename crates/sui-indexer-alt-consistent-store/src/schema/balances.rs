// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bincode::{Decode, Encode};
use sui_indexer_alt_framework::types::{base_types::SuiAddress, TypeTag};

/// Key for the index that supports fetching an account's balance (the sum of balances of all coins
/// it owns).
#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    /// Address of the owner.
    #[bincode(with_serde)]
    pub(crate) owner: SuiAddress,

    /// The coin type e.g. for `0x2::coin::Coin<0x2::sui::SUI>` this would be `0x2::sui::SUI`.
    #[bincode(with_serde)]
    pub(crate) type_: TypeTag,
}

/// Options for creating this index's column family in RocksDB.
pub(crate) fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    let mut opts = base_options.clone();
    opts.set_merge_operator_associative("merge_balances", merge_balances);
    opts.set_compaction_filter("compact_balances", compact_balances);
    opts
}

fn merge_balances(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    // Panic if the merge fails, to prevent us from writing out an invalid state.
    let mut sum: i128 = existing_val.map_or(0, |v| {
        bcs::from_bytes(v).expect("failed to deserialize balance")
    });

    for rand in operands {
        let delta: i128 = bcs::from_bytes(rand).expect("failed to deserialize delta");
        sum += delta;
    }

    Some(bcs::to_bytes(&sum).expect("failed to serialize balance"))
}

fn compact_balances(_level: u32, _key: &[u8], value: &[u8]) -> rocksdb::CompactionDecision {
    // If the balance is zero, we can drop this entry, otherwise keep it.
    if value.iter().all(|b| *b == 0) {
        rocksdb::CompactionDecision::Remove
    } else {
        rocksdb::CompactionDecision::Keep
    }
}
