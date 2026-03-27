// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests three-level argument forwarding — deeper parent chains.
module A::m {
    macro fun a($x: u64): u64 {
        $x + 1
    }

    macro fun b($x: u64): u64 {
        a!($x)
    }

    macro fun c($x: u64): u64 {
        b!($x)
    }

    public fun test(v: u64): u64 {
        c!(v)
    }
}
