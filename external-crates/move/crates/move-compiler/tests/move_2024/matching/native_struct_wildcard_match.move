// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::M {
    public native struct S;

    fun f(s: &S): u64 {
        match (s) {
            _ => 0,
        }
    }
}
