// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#snippet_2
use sui::table::Table;

public struct ProcessedKeys has key {
    id: UID,
    keys: Table<String, bool>,
}

public fun assert_not_processed(store: &ProcessedKeys, key: &String) {
    assert!(!store.keys.contains(key), EDuplicateOperation);
}

public fun mark_processed(store: &mut ProcessedKeys, key: String) {
    store.keys.add(key, true);
}
// docs::/snippet_2
