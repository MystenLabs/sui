// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: native struct nested inside a regular struct, destructured in match.
// Targets: struct_fields().unwrap() in deeper specialization paths.
module 0x0::M {
    public native struct N has drop;

    public struct Wrapper has drop {
        inner: N,
    }

    fun f(w: Wrapper): u64 {
        match (w) {
            Wrapper { inner: N {} } => 1,
        }
    }
}
