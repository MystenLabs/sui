// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A constant dependency cycle across modules is a module dependency cycle

module 0x42::a {

use 0x42::b;

public(package) const A: u64 = b::B + 1;

}

module 0x42::b {

use 0x42::a;

public(package) const B: u64 = a::A + 1;
}
