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
    const CAST_UP_NEG: i64 = (-1i8 as i64);
    const CAST_DOWN_NEG: i8 = (-42i64 as i8);

    // Division and modulus with negative operands truncate toward zero (matching the VM):
    // -7 / 2 == -3 (not floor's -4), -7 % 2 == -1 (not 1), 7 / -2 == -3, 7 % -2 == 1.
    const DIV_NEG_LHS: i8 = -7i8 / 2i8;
    const MOD_NEG_LHS: i8 = -7i8 % 2i8;
    const DIV_NEG_RHS: i8 = 7i8 / -2i8;
    const MOD_NEG_RHS: i8 = 7i8 % -2i8;
    const DIV_NEG_BOTH: i8 = -7i8 / -2i8;
    const MOD_NEG_BOTH: i8 = -7i8 % -2i8;

    // Truncation-direction pins: if folding ever switched to floor semantics, these would
    // overflow at fold time and this file would fail with "cannot compute constant value".
    // trunc: -7 / 2 == -3, and -3 - 125 == -128 fits; floor's -4 - 125 == -129 would not.
    const DIV_TRUNC_PIN: i8 = (-7i8 / 2i8) - 125i8;
    // trunc: -7 % 2 == -1, and 127 + -1 == 126 fits; floor's 127 + 1 == 128 would not.
    const MOD_TRUNC_PIN: i8 = 127i8 + (-7i8 % 2i8);
    // trunc: 7 % -2 == 1, and 1 + -128 == -127 fits; floor's -1 + -128 == -129 would not.
    const MOD_TRUNC_PIN_RHS: i8 = (7i8 % -2i8) + (-128i8);

    // MIN boundary folds that stay in range
    const MIN_DIV_ONE: i8 = -128i8 / 1i8;
    const MIN_MOD_NEG_TWO: i8 = -128i8 % -2i8;

    // Inferred (unsuffixed) literals fold the same way once the type is known
    const INFERRED_DIV_NEG: i8 = -7 / 2;
    const INFERRED_MOD_NEG: i8 = -7 % 2;

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

    // i256 negative-operand division and modulus (truncation toward zero)
    const DIV_NEG_LHS_I256: i256 = -7i256 / 2i256;
    const MOD_NEG_LHS_I256: i256 = -7i256 % 2i256;
    const DIV_NEG_RHS_I256: i256 = 7i256 / -2i256;
    const MOD_NEG_RHS_I256: i256 = 7i256 % -2i256;
    const DIV_NEG_BOTH_I256: i256 = -7i256 / -2i256;
    const MOD_NEG_BOTH_I256: i256 = -7i256 % -2i256;
}
