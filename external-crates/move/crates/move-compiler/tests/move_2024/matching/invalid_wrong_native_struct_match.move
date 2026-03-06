// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::M {
    public native struct N;

    public struct S has drop {
        x: u64,
    }

    fun f(s: S): u64 {
        match (s) {
            N {} => 1,
        }
    }
}
