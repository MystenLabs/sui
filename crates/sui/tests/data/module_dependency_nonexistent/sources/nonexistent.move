// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nonexistent::nonexistent {

    public entry fun main() {}

    public fun test(): u64 {
        let x: u64 = 0;
        x
    }
}
