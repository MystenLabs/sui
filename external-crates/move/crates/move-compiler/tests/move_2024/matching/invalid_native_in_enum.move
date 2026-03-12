// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: native struct nested inside an enum variant, destructured in match.
// Targets: enum_variant_fields().unwrap() and struct_fields().unwrap() in deeper paths.
module 0x0::M {
    public native struct N;

    public enum E {
        X(N),
        Y(u64),
    }

    fun f(e: E): u64 {
        match (e) {
            E::X(N {}) => 1,
            E::Y(v) => v,
        }
    }
}
