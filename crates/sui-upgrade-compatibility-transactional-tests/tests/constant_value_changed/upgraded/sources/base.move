// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module base::base_module {

    const A: u64 = 1;

    public struct X {
        field0: u64,
        field1: u64,
    }

    public fun public_fun(): u64 { A }
    fun private_fun(): u64 { A }
    entry fun private_entry_fun(_x: u64) { }
}
