// Negative literals in match-pattern position, including each width's MIN (which requires the
// parser to fold the '-' into the literal, mirroring negated literal expressions).
module 0x42::m {
    fun match_i8(x: i8): u64 {
        match (x) {
            -128i8 => 0,
            -1i8 => 1,
            0i8 => 2,
            1i8 => 3,
            127i8 => 4,
            _ => 5,
        }
    }

    // MIN via a hex magnitude
    fun match_i8_hex_min(x: i8): u64 {
        match (x) {
            -0x80i8 => 0,
            _ => 1,
        }
    }

    fun match_i16(x: i16): u64 {
        match (x) {
            -32768i16 => 0,
            -1i16 => 1,
            _ => 2,
        }
    }

    fun match_i32(x: i32): u64 {
        match (x) {
            -2147483648i32 => 0,
            -1i32 => 1,
            _ => 2,
        }
    }

    fun match_i64(x: i64): u64 {
        match (x) {
            -9223372036854775808i64 => 0,
            -42i64 => 1,
            _ => 2,
        }
    }

    fun match_i128(x: i128): u64 {
        match (x) {
            -170141183460469231731687303715884105728i128 => 0,
            -1i128 => 1,
            _ => 2,
        }
    }

    fun match_i256(x: i256): u64 {
        match (x) {
            -57896044618658097711785492504343953926634992332820282019728792003956564819968i256 =>
                0,
            -1i256 => 1,
            _ => 2,
        }
    }

    // Or-patterns, at-patterns, and guards alongside negative literal patterns
    fun match_combinators(x: i8): u64 {
        match (x) {
            -1i8 | -2i8 => 0,
            y @ -3i8 => {
                let _ = y;
                1
            },
            n if (*n < 0i8) => 2,
            _ => 3,
        }
    }

    public struct Pair(i8, i64) has copy, drop;

    // Negative literal patterns in nested (constructor) positions
    fun match_nested(p: Pair): u64 {
        match (p) {
            Pair(-128i8, -1i64) => 0,
            Pair(-1i8, _) => 1,
            Pair(_, _) => 2,
        }
    }
}
