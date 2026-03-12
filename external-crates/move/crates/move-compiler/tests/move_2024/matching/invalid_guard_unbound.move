// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: guard expression referencing a variable not bound in the pattern.
// Targets: binder_map.get(&pat_var).unwrap() in translate.rs:2401.
module 0x0::M {
    public enum E has drop {
        A(u64),
        B,
    }

    fun f(e: E): u64 {
        match (e) {
            E::A(x) if (z > 0) => x,
            E::B => 0,
        }
    }
}
