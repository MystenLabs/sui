// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unsuffixed negative literals whose magnitude exceeds `|MIN|` should be rejected,
// with a fix-it that suggests the smallest signed type that fits the negated magnitude.
module 0x42::m {
    fun overflow_i8()   { let _x: i8   = -129; }
    fun overflow_i16()  { let _x: i16  = -32769; }
    fun overflow_i32()  { let _x: i32  = -2147483649; }
    fun overflow_i64()  { let _x: i64  = -9223372036854775809; }
    fun overflow_i128() { let _x: i128 = -170141183460469231731687303715884105729; }
    fun overflow_i256() { let _x: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819969; }
}
