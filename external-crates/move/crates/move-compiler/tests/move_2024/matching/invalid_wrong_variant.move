// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: pattern using a variant name that doesn't exist on the enum.
// Targets: enum_variant_fields().unwrap() paths.
module 0x0::M {
    public enum E {
        A,
        B,
    }

    fun f(e: E): u64 {
        match (e) {
            E::A => 1,
            E::C => 2,
        }
    }
}
