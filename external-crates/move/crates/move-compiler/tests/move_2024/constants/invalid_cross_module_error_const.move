// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// '#[error]' constants cannot be used in abort positions outside their defining module, since
// the error information is encoded against the aborting module's tables

module 0x42::a {

#[error]
public(package) const ENotFound: vector<u8> = b"not found";

}

module 0x42::b {

use 0x42::a;

public fun fail() {
    abort a::ENotFound
}

public fun check(cond: bool) {
    assert!(cond, a::ENotFound);
}
}
