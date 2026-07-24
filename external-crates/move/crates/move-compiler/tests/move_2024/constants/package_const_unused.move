// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A 'public(package)' constant used nowhere in the package still warns as unused; one used
// only from another module does not

module 0x42::a {

public(package) const UNUSED: u64 = 100;
public(package) const USED: u64 = 7;

}

module 0x42::b {

use 0x42::a;

public fun used(): u64 { a::USED }

}
