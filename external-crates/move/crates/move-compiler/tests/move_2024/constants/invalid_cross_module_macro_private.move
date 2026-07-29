// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A macro body referencing its module's private constant errors when expanded in another
// module: visibility is resolved in the scope of the caller

module 0x42::a {

const SECRET: u64 = 42;

public macro fun get_secret(): u64 {
    SECRET
}

}

module 0x42::b {

use 0x42::a;

public fun steal(): u64 { a::get_secret!() }

}
