// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A constant reference in a macro body is a cross-module use when the macro expands in another
// module: the getter must be synthesized even though no cross-module reference appears in the
// source of the defining module

module 0x42::a {

public(package) const LIMIT: u64 = 10;

public macro fun clamp($x: u64): u64 {
    let x = $x;
    if (x > LIMIT) LIMIT else x
}

}

module 0x42::b {

use 0x42::a;

public fun clamped(x: u64): u64 {
    a::clamp!(x)
}

}
