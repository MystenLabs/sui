// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Member completion on a module with a generated constant function lists the constant but not
// the generated function; a rejected (private) cross-module use under IDE mode continues past
// the error without an ICE

module 0x42::a {

public(package) const MAX: u64 = 100;

const SECRET: u64 = 7;

public fun secret(): u64 { SECRET }

}

module 0x42::b {

use 0x42::a;

public fun max(): u64 { a::MAX }

public fun steal(): u64 { a::SECRET }

public fun complete(): u64 {
    a::
}

}
