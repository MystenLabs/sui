// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An '#[error]' constant used in an abort position inside a macro is rejected when the macro
// expands in another module: the abort executes in the caller, whose tables cannot encode the
// defining module's constant

module 0x42::a {

#[error]
public(package) const ENotValid: vector<u8> = b"invalid";

public macro fun check_valid($x: u64) {
    assert!($x < 10, ENotValid);
}

}

module 0x42::b {

use 0x42::a;

public fun check(x: u64) {
    a::check_valid!(x)
}

}
