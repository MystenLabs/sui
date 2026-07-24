// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Cross-module constants of every constant type, in constant definitions and function bodies

module 0x42::a {

public(package) const COUNT: u64 = 100;
public(package) const ADDR: address = @0x7;
public(package) const FLAG: bool = true;
public(package) const SMALL: u8 = 255;
public(package) const BIG: u128 = 340282366920938463463374607431768211455;
public(package) const HUGE: u256 =
    115792089237316195423570985008687907853269984665640564039457584007913129639935;
public(package) const NESTED: vector<vector<u8>> = vector[b"a", b"bc"];

}

module 0x42::b {

use 0x42::a;

const ADDR2: address = a::ADDR;
const HALF: u128 = a::BIG / 2;
const VECS: vector<vector<vector<u8>>> = vector[a::NESTED, a::NESTED];

public fun addr(): address { a::ADDR }
public fun flag(): bool { a::FLAG }
public fun huge(): u256 { a::HUGE }
public fun nested(): vector<vector<u8>> { a::NESTED }
public fun in_vector(): vector<u8> { vector[a::SMALL, a::SMALL, 0] }
public fun folded(): (address, u128, vector<vector<vector<u8>>>) { (ADDR2, HALF, VECS) }

public fun in_assert(x: u64) { assert!(x < a::COUNT, 0) }

public fun discarded() { a::COUNT; }

public fun after_abort(): u64 { abort 0; a::COUNT }

}
