//# init --edition development

//# run
module 0x42::m {
    // ---- i8 constants ----
    const I8_MIN: i8 = -128i8;
    const I8_MAX: i8 = 127i8;
    const I8_ZERO: i8 = 0i8;
    const I8_NEG_LIT: i8 = -1i8;

    // Folded arithmetic
    const I8_ADD: i8 = 10i8 + -20i8;
    const I8_SUB: i8 = -50i8 - -30i8;
    const I8_MUL: i8 = -6i8 * 7i8;
    const I8_DIV: i8 = 42i8 / -6i8;
    const I8_MOD: i8 = -7i8 % 3i8;

    // Boundary + 0
    const I8_MAX_ADD0: i8 = 127i8 + 0i8;
    const I8_MIN_ADD0: i8 = -128i8 + 0i8;

    // Bitwise
    const I8_BAND: i8 = 0x7Fi8 & 0x0Fi8;
    const I8_BOR: i8 = 0x70i8 | 0x0Fi8;
    const I8_BXOR: i8 = 0x7Fi8 ^ 0x7Fi8;

    // Shift
    const I8_SHL: i8 = 1i8 << 6;
    const I8_SHR: i8 = 64i8 >> 6;

    // ---- i16 constants ----
    const I16_MIN: i16 = -32768i16;
    const I16_MAX: i16 = 32767i16;
    const I16_ADD: i16 = 100i16 + -200i16;
    const I16_SUB: i16 = -500i16 - -300i16;
    const I16_MUL: i16 = -60i16 * 70i16;
    const I16_DIV: i16 = 420i16 / -6i16;
    const I16_MOD: i16 = -17i16 % 5i16;
    const I16_MAX_ADD0: i16 = 32767i16 + 0i16;
    const I16_MIN_ADD0: i16 = -32768i16 + 0i16;
    const I16_BAND: i16 = 0x7FFFi16 & 0x00FFi16;
    const I16_BOR: i16 = 0x7F00i16 | 0x00FFi16;
    const I16_BXOR: i16 = 0x7FFFi16 ^ 0x7FFFi16;
    const I16_SHL: i16 = 1i16 << 14;
    const I16_SHR: i16 = 16384i16 >> 14;

    // ---- i32 constants ----
    const I32_MIN: i32 = -2147483648i32;
    const I32_MAX: i32 = 2147483647i32;
    const I32_ADD: i32 = 1000i32 + -2000i32;
    const I32_SUB: i32 = -5000i32 - -3000i32;
    const I32_MUL: i32 = -600i32 * 700i32;
    const I32_DIV: i32 = 4200i32 / -6i32;
    const I32_MOD: i32 = -17i32 % 5i32;
    const I32_MAX_ADD0: i32 = 2147483647i32 + 0i32;
    const I32_MIN_ADD0: i32 = -2147483648i32 + 0i32;
    const I32_BAND: i32 = 0x7FFFFFFFi32 & 0x0000FFFFi32;
    const I32_BOR: i32 = 0x7FFF0000i32 | 0x0000FFFFi32;
    const I32_BXOR: i32 = 0x7FFFFFFFi32 ^ 0x7FFFFFFFi32;
    const I32_SHL: i32 = 1i32 << 30;
    const I32_SHR: i32 = 1073741824i32 >> 30;

    // ---- i64 constants ----
    const I64_MIN: i64 = -9223372036854775808i64;
    const I64_MAX: i64 = 9223372036854775807i64;
    const I64_ADD: i64 = 10000i64 + -20000i64;
    const I64_SUB: i64 = -50000i64 - -30000i64;
    const I64_MUL: i64 = -6000i64 * 7000i64;
    const I64_DIV: i64 = 42000i64 / -6i64;
    const I64_MOD: i64 = -17i64 % 5i64;
    const I64_MAX_ADD0: i64 = 9223372036854775807i64 + 0i64;
    const I64_MIN_ADD0: i64 = -9223372036854775808i64 + 0i64;
    const I64_BAND: i64 = 0x7FFFFFFFFFFFFFFFi64 & 0x00000000FFFFFFFFi64;
    const I64_BOR: i64 = 0x7FFFFFFF00000000i64 | 0x00000000FFFFFFFFi64;
    const I64_BXOR: i64 = 0x7FFFFFFFFFFFFFFFi64 ^ 0x7FFFFFFFFFFFFFFFi64;
    const I64_SHL: i64 = 1i64 << 62;
    const I64_SHR: i64 = 4611686018427387904i64 >> 62;

    // ---- i128 constants ----
    const I128_MIN: i128 = -170141183460469231731687303715884105728i128;
    const I128_MAX: i128 = 170141183460469231731687303715884105727i128;
    const I128_ADD: i128 = 100000i128 + -200000i128;
    const I128_SUB: i128 = -500000i128 - -300000i128;
    const I128_MUL: i128 = -60000i128 * 70000i128;
    const I128_DIV: i128 = 420000i128 / -6i128;
    const I128_MOD: i128 = -17i128 % 5i128;
    const I128_MAX_ADD0: i128 = 170141183460469231731687303715884105727i128 + 0i128;
    const I128_MIN_ADD0: i128 = -170141183460469231731687303715884105728i128 + 0i128;
    const I128_BXOR: i128 = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi128 ^ 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi128;
    const I128_SHL: i128 = 1i128 << 126;
    const I128_SHR: i128 = 85070591730234615865843651857942052864i128 >> 126;

    // ---- i256 constants ----
    const I256_MIN: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819968i256;
    const I256_MAX: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819967i256;
    const I256_ADD: i256 = 1000000i256 + -2000000i256;
    const I256_SUB: i256 = -5000000i256 - -3000000i256;
    const I256_MUL: i256 = -600000i256 * 700000i256;
    const I256_DIV: i256 = 4200000i256 / -6i256;
    const I256_MOD: i256 = -17i256 % 5i256;
    const I256_MAX_ADD0: i256 = 57896044618658097711785492504343953926634992332820282019728792003956564819967i256 + 0i256;
    const I256_MIN_ADD0: i256 = -57896044618658097711785492504343953926634992332820282019728792003956564819968i256 + 0i256;
    const I256_BXOR: i256 = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi256 ^ 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFi256;
    const I256_SHL: i256 = 1i256 << 254;
    const I256_SHR: i256 = 28948022309329048855892746252171976963317496166410141009864396001978282409984i256 >> 254;

    fun main() {
        // i8 assertions
        assert!(I8_MIN == -128i8, 42);
        assert!(I8_MAX == 127i8, 42);
        assert!(I8_ZERO == 0i8, 42);
        assert!(I8_NEG_LIT == -1i8, 42);
        assert!(I8_ADD == -10i8, 42);
        assert!(I8_SUB == -20i8, 42);
        assert!(I8_MUL == -42i8, 42);
        assert!(I8_DIV == -7i8, 42);
        assert!(I8_MOD == -1i8, 42);
        assert!(I8_MAX_ADD0 == 127i8, 42);
        assert!(I8_MIN_ADD0 == -128i8, 42);
        assert!(I8_BAND == 0x0Fi8, 42);
        assert!(I8_BOR == 0x7Fi8, 42);
        assert!(I8_BXOR == 0i8, 42);
        assert!(I8_SHL == 64i8, 42);
        assert!(I8_SHR == 1i8, 42);

        // i16 assertions
        assert!(I16_MIN == -32768i16, 42);
        assert!(I16_MAX == 32767i16, 42);
        assert!(I16_ADD == -100i16, 42);
        assert!(I16_SUB == -200i16, 42);
        assert!(I16_MUL == -4200i16, 42);
        assert!(I16_DIV == -70i16, 42);
        assert!(I16_MOD == -2i16, 42);
        assert!(I16_MAX_ADD0 == 32767i16, 42);
        assert!(I16_MIN_ADD0 == -32768i16, 42);
        assert!(I16_BAND == 0x00FFi16, 42);
        assert!(I16_BOR == 0x7FFFi16, 42);
        assert!(I16_BXOR == 0i16, 42);
        assert!(I16_SHL == 16384i16, 42);
        assert!(I16_SHR == 1i16, 42);

        // i32 assertions
        assert!(I32_MIN == -2147483648i32, 42);
        assert!(I32_MAX == 2147483647i32, 42);
        assert!(I32_ADD == -1000i32, 42);
        assert!(I32_SUB == -2000i32, 42);
        assert!(I32_MUL == -420000i32, 42);
        assert!(I32_DIV == -700i32, 42);
        assert!(I32_MOD == -2i32, 42);
        assert!(I32_MAX_ADD0 == 2147483647i32, 42);
        assert!(I32_MIN_ADD0 == -2147483648i32, 42);
        assert!(I32_BAND == 0x0000FFFFi32, 42);
        assert!(I32_BOR == 0x7FFFFFFFi32, 42);
        assert!(I32_BXOR == 0i32, 42);
        assert!(I32_SHL == 1073741824i32, 42);
        assert!(I32_SHR == 1i32, 42);

        // i64 assertions
        assert!(I64_MIN == -9223372036854775808i64, 42);
        assert!(I64_MAX == 9223372036854775807i64, 42);
        assert!(I64_ADD == -10000i64, 42);
        assert!(I64_SUB == -20000i64, 42);
        assert!(I64_MUL == -42000000i64, 42);
        assert!(I64_DIV == -7000i64, 42);
        assert!(I64_MOD == -2i64, 42);
        assert!(I64_MAX_ADD0 == 9223372036854775807i64, 42);
        assert!(I64_MIN_ADD0 == -9223372036854775808i64, 42);
        assert!(I64_BAND == 0x00000000FFFFFFFFi64, 42);
        assert!(I64_BOR == 0x7FFFFFFFFFFFFFFFi64, 42);
        assert!(I64_BXOR == 0i64, 42);
        assert!(I64_SHL == 4611686018427387904i64, 42);
        assert!(I64_SHR == 1i64, 42);

        // i128 assertions
        assert!(I128_MIN == -170141183460469231731687303715884105728i128, 42);
        assert!(I128_MAX == 170141183460469231731687303715884105727i128, 42);
        assert!(I128_ADD == -100000i128, 42);
        assert!(I128_SUB == -200000i128, 42);
        assert!(I128_MUL == -4200000000i128, 42);
        assert!(I128_DIV == -70000i128, 42);
        assert!(I128_MOD == -2i128, 42);
        assert!(I128_MAX_ADD0 == 170141183460469231731687303715884105727i128, 42);
        assert!(I128_MIN_ADD0 == -170141183460469231731687303715884105728i128, 42);
        assert!(I128_BXOR == 0i128, 42);
        assert!(I128_SHL == 85070591730234615865843651857942052864i128, 42);
        assert!(I128_SHR == 1i128, 42);

        // i256 assertions
        assert!(I256_MIN == -57896044618658097711785492504343953926634992332820282019728792003956564819968i256, 42);
        assert!(I256_MAX == 57896044618658097711785492504343953926634992332820282019728792003956564819967i256, 42);
        assert!(I256_ADD == -1000000i256, 42);
        assert!(I256_SUB == -2000000i256, 42);
        assert!(I256_MUL == -420000000000i256, 42);
        assert!(I256_DIV == -700000i256, 42);
        assert!(I256_MOD == -2i256, 42);
        assert!(I256_MAX_ADD0 == 57896044618658097711785492504343953926634992332820282019728792003956564819967i256, 42);
        assert!(I256_MIN_ADD0 == -57896044618658097711785492504343953926634992332820282019728792003956564819968i256, 42);
        assert!(I256_BXOR == 0i256, 42);
        assert!(I256_SHL == 28948022309329048855892746252171976963317496166410141009864396001978282409984i256, 42);
        assert!(I256_SHR == 1i256, 42);
    }
}
