// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests: struct pattern for a different struct than the subject type.
// Targets: type mismatch in match patterns, struct_fields paths.
module 0x0::M {
    public struct S has drop { x: u64 }
    public struct T has drop { y: u64 }

    fun f(s: S): u64 {
        match (s) {
            T { y } => y,
        }
    }
}
