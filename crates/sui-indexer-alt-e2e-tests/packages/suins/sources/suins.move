// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A mock of the SuiNS Move package to use during testing.
///
/// It provides enough structure to query SuiNS's on-chain state, but it
/// doesn't maintain the same invariants.
module suins::suins;

use std::string::String;
use sui::table::{Self, Table};
use sui::vec_map::{Self, VecMap};

use suins::domain::{Self, Domain};

public struct NameRecord has copy, drop, store {
    nft_id: ID,
    expiration_timestamp_ms: u64,
    target_address: Option<address>,
    data: VecMap<String, String>,
}

public fun share_forward_registry(ctx: &mut TxContext) {
  let registry: Table<Domain, NameRecord> = table::new(ctx);
  transfer::public_share_object(registry)
}

public fun share_reverse_registry(ctx: &mut TxContext) {
  let registry: Table<address, Domain> = table::new(ctx);
  transfer::public_share_object(registry)
}

public fun add_domain(
    forward_registry: &mut Table<Domain, NameRecord>,
    reverse_registry: &mut Table<address, Domain>,
    nft_id: ID,
    labels: vector<String>,
    target_address: Option<address>,
    expiration_timestamp_ms: u64,
) {
    let domain = domain::new(labels);

    let name_record = NameRecord {
        nft_id,
        expiration_timestamp_ms,
        target_address,
        data: vec_map::empty(),
    };

    forward_registry.add(domain, name_record);
    target_address.do!(|t| reverse_registry.add(t, domain));
}
