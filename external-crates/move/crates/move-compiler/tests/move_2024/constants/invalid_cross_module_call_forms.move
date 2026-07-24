// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Constants cannot be called as functions, and the generated constant functions cannot be
// named from source (they do not exist until after typing)

module 0x42::a {

public(package) const MAX: u64 = 100;

public fun touch(): u64 { MAX }

}

module 0x42::b {

use 0x42::a;

public fun call_const(): u64 { a::MAX() }

public fun call_generated(): u64 { a::_const_MAX() }

public fun use_it(): u64 { a::MAX }

}
