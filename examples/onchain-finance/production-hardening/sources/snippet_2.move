// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::idempotency;

use std::string::String;
use sui::table::Table;

// docs::#onchain-idempotency
public struct ProcessedKeys has key {
    id: UID,
    keys: Table<String, bool>,
}

public fun is_processed(store: &ProcessedKeys, key: &String): bool {
    store.keys.contains(key)
}

public fun mark_processed(store: &mut ProcessedKeys, key: String) {
    store.keys.add(key, true);
}
// docs::/#onchain-idempotency
