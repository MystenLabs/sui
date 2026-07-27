// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#directory
module example::directory;

use sui::table::{Self, Table};

public struct Directory has key {
    id: UID,
    entries: Table<vector<u8>, address>,
}
// docs::/#directory
