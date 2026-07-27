// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Constants from modules whose names would collide under module-name-only mangling
// (`x::A_B` and `x_A::B` would both be `_x_A_B`) get distinct copy names, since the mangling
// leads with the defining module's dependency order

module 0x42::x {

public(package) const A_B: u64 = 1;

}

module 0x42::x_A {

public(package) const B: u64 = 2;

}

module 0x42::user {

use 0x42::x;
use 0x42::x_A;

public fun both(): u64 {
    x::A_B + x_A::B
}

}
