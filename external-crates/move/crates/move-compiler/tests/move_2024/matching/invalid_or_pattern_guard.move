// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: or-pattern combined with a guard.
// Targets: or-pattern flattening and guard binder paths.
module 0x0::M {
    public enum E has drop {
        A(u64),
        B(u64),
        C,
    }

    fun f(e: E): u64 {
        match (e) {
            E::A(x) | E::B(x) if (x > 0) => x,
            _ => 0,
        }
    }
}
