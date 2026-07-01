// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    // Constant folding for signed arithmetic
    const ADD: i64 = 10i64 + 20i64;
    const SUB: i64 = 50i64 - 30i64;
    const MUL: i32 = 6i32 * 7i32;
    const DIV: i16 = 100i16 / 10i16;
    const MOD: i8 = 10i8 % 3i8;

    // Constant folding with negation
    const NEG: i64 = -(42i64);
    const DOUBLE_NEG: i64 = -(-(42i64));
    const NEG_ZERO: i64 = -(0i64);

    // Constant folding with nested expressions
    const NESTED: i64 = (10i64 + 20i64) * 3i64;
    const NESTED2: i32 = -(5i32 + 10i32);

    // Constant folding with comparison
    const CMP: bool = 10i64 > 5i64;
    const CMP2: bool = 1i8 == 1i8;
    const CMP3: bool = 10i32 <= 10i32;
    const CMP4: bool = 1i16 != 2i16;

    // Constant folding with bitwise
    const BAND: i64 = 0x0Fi64 & 0x03i64;
    const BOR: i64 = 0x0Fi64 | 0x30i64;
    const BXOR: i64 = 0x0Fi64 ^ 0x03i64;

    // Constant folding with shift
    const SHL: i64 = 1i64 << 8u8;
    const SHR: i64 = 256i64 >> 4u8;

    // Constant folding with cast between signed types
    const CAST_UP: i64 = (1i8 as i64);
    const CAST_DOWN: i8 = (42i64 as i8);

    // i256 constant folding
    const ADD_I256: i256 = 10i256 + 20i256;
    const SUB_I256: i256 = 50i256 - 30i256;
    const MUL_I256: i256 = 6i256 * 7i256;
    const DIV_I256: i256 = 100i256 / 10i256;
    const MOD_I256: i256 = 10i256 % 3i256;
    const NEG_I256: i256 = -(42i256);
    const CMP_I256: bool = 10i256 > 5i256;
    const BAND_I256: i256 = 0x0Fi256 & 0x03i256;
    const BOR_I256: i256 = 0x0Fi256 | 0x30i256;
    const BXOR_I256: i256 = 0x0Fi256 ^ 0x03i256;
    const SHL_I256: i256 = 1i256 << 8u8;
    const SHR_I256: i256 = 256i256 >> 4u8;
    const CAST_I256: i256 = (1i8 as i256);
    const CAST_FROM_I256: i8 = (42i256 as i8);
}
