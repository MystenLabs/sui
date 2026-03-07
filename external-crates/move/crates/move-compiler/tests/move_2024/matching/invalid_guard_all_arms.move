// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: every arm has a guard, no unguarded fallthrough.
// Targets: make_leaf guard logic at lines 559/570 in match_compilation.
module 0x0::M {
    public enum E has drop {
        A(u64),
        B(u64),
    }

    fun f(e: E): u64 {
        match (e) {
            E::A(x) if (x > 10) => x + 1,
            E::B(x) if (x > 5) => x + 2,
        }
    }
}
