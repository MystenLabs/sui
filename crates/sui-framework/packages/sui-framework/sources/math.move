// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic math for nicer programmability
module sui::math {

    /// Return the larger of `x` and `y`
    public fun max(x: u64, y: u64): u64 {
        if (x > y) {
            x
        } else {
            y
        }
    }

    /// Return the smaller of `x` and `y`
    public fun min(x: u64, y: u64): u64 {
        if (x < y) {
            x
        } else {
            y
        }
    }

    public fun min_u256(x: u256, y: u256): u256 {
        if (x < y) {
            x
        } else {
            y
        }
    }

    /// Return the absolute value of x - y
    public fun diff(x: u64, y: u64): u64 {
        if (x > y) {
            x - y
        } else {
            y - x
        }
    }

    /// Return the value of a base raised to a power
    public fun pow(mut base: u64, mut exponent: u8): u64 {
        let mut res = 1;
        while (exponent >= 1) {
            if (exponent % 2 == 0) {
                base = base * base;
                exponent = exponent / 2;
            } else {
                res = res * base;
                exponent = exponent - 1;
            }
        };

        res
    }

    /// Get a nearest lower integer Square Root for `x`. Given that this
    /// function can only operate with integers, it is impossible
    /// to get perfect (or precise) integer square root for some numbers.
    ///
    /// Example:
    /// ```
    /// math::sqrt(9) => 3
    /// math::sqrt(8) => 2 // the nearest lower square root is 4;
    /// ```
    ///
    /// In integer math, one of the possible ways to get results with more
    /// precision is to use higher values or temporarily multiply the
    /// value by some bigger number. Ideally if this is a square of 10 or 100.
    ///
    /// Example:
    /// ```
    /// math::sqrt(8) => 2;
    /// math::sqrt(8 * 10000) => 282;
    /// // now we can use this value as if it was 2.82;
    /// // but to get the actual result, this value needs
    /// // to be divided by 100 (because sqrt(10000)).
    ///
    ///
    /// math::sqrt(8 * 1000000) => 2828; // same as above, 2828 / 1000 (2.828)
    /// ```
    public fun sqrt(x: u64): u64 {
        let mut bit = 1u128 << 64;
        let mut res = 0u128;
        let mut x = x as u128;

        while (bit != 0) {
            if (x >= res + bit) {
                x = x - (res + bit);
                res = (res >> 1) + bit;
            } else {
                res = res >> 1;
            };
            bit = bit >> 2;
        };

        res as u64
    }

    /// Similar to math::sqrt, but for u128 numbers. Get a nearest lower integer Square Root for `x`. Given that this
    /// function can only operate with integers, it is impossible
    /// to get perfect (or precise) integer square root for some numbers.
    ///
    /// Example:
    /// ```
    /// math::sqrt_u128(9) => 3
    /// math::sqrt_u128(8) => 2 // the nearest lower square root is 4;
    /// ```
    ///
    /// In integer math, one of the possible ways to get results with more
    /// precision is to use higher values or temporarily multiply the
    /// value by some bigger number. Ideally if this is a square of 10 or 100.
    ///
    /// Example:
    /// ```
    /// math::sqrt_u128(8) => 2;
    /// math::sqrt_u128(8 * 10000) => 282;
    /// // now we can use this value as if it was 2.82;
    /// // but to get the actual result, this value needs
    /// // to be divided by 100 (because sqrt_u128(10000)).
    ///
    ///
    /// math::sqrt_u128(8 * 1000000) => 2828; // same as above, 2828 / 1000 (2.828)
    /// ```
    public fun sqrt_u128(x: u128): u128 {
        let mut bit = 1u256 << 128;
        let mut res = 0u256;
        let mut x = x as u256;

        while (bit != 0) {
            if (x >= res + bit) {
                x = x - (res + bit);
                res = (res >> 1) + bit;
            } else {
                res = res >> 1;
            };
            bit = bit >> 2;
        };

        res as u128
    }

    /// Calculate x / y, but round up the result.
    public fun divide_and_round_up(x: u64, y: u64): u64 {
        if (x % y == 0) {
            x / y
        } else {
            x / y + 1
        }
    }

    public fun log2_u256(mut x: u256): u8 {
        let mut result = 0;
        if (x >> 128 > 0) {
            x = x >> 128;
            result = result + 128;
        };

        if (x >> 64 > 0) {
            x = x >> 64;
            result = result + 64;
        };

        if (x >> 32 > 0) {
            x = x >> 32;
            result = result + 32;
        };

        if (x >> 16 > 0) {
            x = x >> 16;
            result = result + 16;
        };

        if (x >> 8 > 0) {
            x = x >> 8;
            result = result + 8;
        };

        if (x >> 4 > 0) {
            x = x >> 4;
            result = result + 4;
        };

        if (x >> 2 > 0) {
            x = x >> 2;
            result = result + 2;
        };

        if (x >> 1 > 0)
            result = result + 1;

        result
    }

    public fun sqrt_u256(x: u256): u256 {
        if (x == 0) return 0;

        let mut result = 1 << ((log2_u256(x) >> 1) as u8);

        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;
        result = (result + x / result) >> 1;

        min_u256(result, x / result)
    }

}
