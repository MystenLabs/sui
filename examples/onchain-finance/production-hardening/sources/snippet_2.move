// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::idempotency;

use std::string::String;
use sui::table::{Self, Table};

#[error(code = 0)]
const EDuplicateKey: vector<u8> = b"Idempotency key is already processed";

// docs::#onchain-idempotency
public struct ProcessedKeys has key {
    id: UID,
    keys: Table<String, bool>,
}

fun init(ctx: &mut TxContext) {
    transfer::share_object(ProcessedKeys {
        id: object::new(ctx),
        keys: table::new(ctx),
    });
}

public fun is_processed(store: &ProcessedKeys, key: String): bool {
    store.keys.contains(key)
}

/// Keep this call in the same transaction as the protected operation.
public fun mark_processed(store: &mut ProcessedKeys, key: String) {
    assert!(!store.keys.contains(key), EDuplicateKey);
    store.keys.add(key, true);
}
// docs::/#onchain-idempotency
