// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A constant without 'public(package)' visibility cannot be used from another module

module 0x42::a {

const MAX: u64 = 100;

public fun max(): u64 { MAX }

}

module 0x42::b {

use 0x42::a;

const DOUBLE: u64 = a::MAX * 2;

public fun limit(): u64 {
    a::MAX
}

public fun double(): u64 { DOUBLE }

}
