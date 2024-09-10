// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base::base_module {
    public struct X {
        field0: u64,
        field1: u64,
    }

    public fun renamed_public_fun(): u64 { 0 }
    fun private_fun(): u64 { 0 }
    entry fun private_entry_fun(_x: u64) { }
}
