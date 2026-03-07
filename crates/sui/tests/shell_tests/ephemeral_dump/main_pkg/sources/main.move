// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module main_pkg::main {
    use dep_pkg::dep;

    public fun call_dep(): u64 {
        dep::hello()
    }
}
