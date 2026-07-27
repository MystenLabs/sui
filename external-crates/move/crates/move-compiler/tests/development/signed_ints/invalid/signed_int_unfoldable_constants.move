// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Signed constant expressions that would abort at runtime are deliberately not folded, so in a
// 'const' they surface as "cannot compute constant value" errors (the signed counterpart to
// move_check/folding/unfoldable_constants.move).
module 0x42::m {
    // Division and modulus by zero
    const DIV0: i8 = 1i8 / 0i8;
    const DIV1: i64 = 1i64 / 0i64;
    const DIV2: i256 = 1i256 / 0i256;
    const MOD0: i8 = 1i8 % 0i8;
    const MOD1: i64 = 1i64 % 0i64;
    const MOD2: i256 = 1i256 % 0i256;

    // MIN / -1 and MIN % -1 overflow
    const DIVMIN0: i8 = -128i8 / -1i8;
    const DIVMIN1: i64 = -9223372036854775808i64 / -1i64;
    const DIVMIN2: i256 =
        -57896044618658097711785492504343953926634992332820282019728792003956564819968i256
            / -1i256;
    const MODMIN0: i8 = -128i8 % -1i8;
    const MODMIN1: i64 = -9223372036854775808i64 % -1i64;
    const MODMIN2: i256 =
        -57896044618658097711785492504343953926634992332820282019728792003956564819968i256
            % -1i256;

    // Arithmetic overflow
    const ADD0: i8 = 127i8 + 1i8;
    const SUB0: i8 = -128i8 - 1i8;
    const MUL0: i8 = 64i8 * 2i8;

    // Negation overflow: -(MIN) does not fit
    const NEG0: i8 = -(-128i8);

    // Signed left shifts that change the sign bit or discard significant bits are not folded
    // (the VM computes them by wrapping at runtime, e.g. 1i8 << 7 == -128)
    const SHL0: i8 = 1i8 << 7u8;
    // Shifts by >= bit width abort at runtime
    const SHL1: i8 = 1i8 << 8u8;
    const SHR0: i8 = 1i8 >> 8u8;

    // Narrowing casts that do not fit
    const CAST0: i8 = (128i64 as i8);
    const CAST1: i8 = (-129i64 as i8);
}
