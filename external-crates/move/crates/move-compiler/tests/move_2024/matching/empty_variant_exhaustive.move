// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests exhaustive matching on single-variant enums and empty-field variants
module 0x42::m;

public enum Single {
    Only,
}

public enum SingleField {
    Only(u64),
}

public enum TwoEmpty {
    A,
    B,
}

fun test_single(s: Single): u64 {
    match (s) {
        Single::Only => 0,
    }
}

fun test_single_field(s: SingleField): u64 {
    match (s) {
        SingleField::Only(x) => x,
    }
}

fun test_two_empty(t: TwoEmpty): u64 {
    match (t) {
        TwoEmpty::A => 1,
        TwoEmpty::B => 2,
    }
}
