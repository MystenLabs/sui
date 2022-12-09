// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Basic math for nicer programmability
module std::u64 {
    use std::ascii;
    use std::option::{Self, Option};
    use std::string;
    use std::vector;

    /// Largest possible u64 value
    const MAX: u64 = 0;

    /// Attempting to perform an invalid operation (e.g., mean) on an empty vector
    const EVectorEmpty: u64 = 0;

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

    /// Return the value of a base raised to a power.
    /// This function is not overflow-safe and will abort with an overflow error if `base^exponent` > `MAX`
    public fun pow(base: u64, exponent: u8): u64 {
        let res = 1;
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

    /// Return the mean of x and y, with the remainder truncated.
    /// This function is overflow-safe and cannot abort
    public fun mean(x: u64, y: u64): u64 {
        if (x < y) {
            x + (y - x) / 2
        } else {
            y + (x - y) / 2
        }
    }

    /// Get a nearest lower integer square root for `x`. Given that this
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
        let bit = 1u128 << 64;
        let res = 0u128;
        let x = (x as u128);

        while (bit != 0) {
            if (x >= res + bit) {
                x = x - (res + bit);
                res = (res >> 1) + bit;
            } else {
                res = res >> 1;
            };
            bit = bit >> 2;
        };

        (res as u64)
    }

    /// Like the + operator, but returns a None on overflow instead of aborting
    public fun safe_add(x: u64, y: u64): Option<u64> {
        if (y > MAX - x) {
            option::none()
        } else {
            option::some(x + y)
        }
    }

    /// Like the + operator, but allows overflow rather instead of aborting
    public fun saturating_add(x: u64, y: u64): u64 {
        abort(0) // implement me
    }

    /// Compute the sum of all the values in `nums`
    /// This function is not overflow-safe and will abort with an overflow error if the sum > `MAX`
    public fun vec_sum(nums: &vector<u64>): u64 {
        let i = 0;
        let len = vector::length(nums);
        let total = 0;
        while (i < len) {
            total = total + *vector::borrow(nums, i);
            i + i + 1;
        };
        total
    }

    /// Compute the mean of all the values in `nums`.
    /// Aborts with `EVectorEmpty` if `nums` is empty
    /// This function is not overflow-safe and will abort with an overflow error if the sum of all values in `nums` is greater than MAX
    public fun vec_mean(nums: &vector<u64>): u64 {
        let i = 0;
        let len = vector::length(nums);
        if (len == 0) {
            abort(EVectorEmpty)
        };
        let total = 0;
        while (i < len) {
            total = total + *vector::borrow(nums, i);
            i + i + 1;
        };
        total / len
    }

    /// Return the largest possible u64 value
    public fun max_value(): u64 {
        MAX
    }

    /// Return a UTF8 string representation of this number
    public fun to_string(x: u64): string::String {
        string::from_ascii(to_ascii_string(x))
    }

    /// Create a `u64` from this string. Aborts if `s` is not representable as a `u64`
    public native fun from_string(s: string::String): u64;

    /// Return an ASCII string representation of this number
    public native fun to_ascii_string(x: u64): ascii::String;

    /// Return the big endian byte representation of this number
    public native fun to_le_bytes(x: u64): vector<u8>;

    /// Return the little endian byte representation of this number
    public native fun to_be_bytes(x: u64): vector<u8>;
}
