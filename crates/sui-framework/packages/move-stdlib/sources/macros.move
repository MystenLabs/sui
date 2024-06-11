
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module holds shared implementation of macros used in `std`
module std::macros {
    public(package) macro fun num_max($x: _, $y: _): _ {
        let x = $x;
        let y = $y;
        if (x > y) x
        else y
    }

    public(package) macro fun num_min($x: _, $y: _): _ {
        let x = $x;
        let y = $y;
        if (x < y) x
        else y
    }

    public(package) macro fun num_diff($x: _, $y: _): _ {
        let x = $x;
        let y = $y;
        if (x > y) x - y
        else y - x
    }

    public(package) macro fun num_divide_and_round_up($x: _, $y: _): _ {
        let x = $x;
        let y = $y;
        if (x % y == 0) x / y
        else x / y + 1
    }


    public(package) macro fun num_pow($base: _, $exponent: u8): _ {
        let mut base = $base;
        let mut exponent = $exponent;
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

    public(package) macro fun num_sqrt<$T, $U>($x: $T, $bitsize: u8): $T {
        let x = $x;
        let mut bit = (1: $U) << $bitsize;
        let mut res = (0: $U);
        let mut x = x as $U;

        while (bit != 0) {
            if (x >= res + bit) {
                x = x - (res + bit);
                res = (res >> 1) + bit;
            } else {
                res = res >> 1;
            };
            bit = bit >> 2;
        };

        res as $T
    }

    public(package) macro fun range_do($start: _, $stop: _, $f: |_|) {
        let mut i = $start;
        let stop = $stop;
        while (i < stop) {
            $f(i);
            i = i + 1;
        }
    }

    public(package) macro fun range_do_eq($start: _, $stop: _, $f: |_|) {
        let mut i = $start;
        let stop = $stop;
        // we check `i >= stop` inside the loop instead of `i <= stop` as `while` condition to avoid
        // incrementing `i` past the MAX integer value.
        // Because of this, we need to check if `i > stop` and return early--instead of letting the
        // loop bound handle it, like in the `range_do` macro.
        if (i > stop) return;
        loop {
            $f(i);
            if (i >= stop) break;
            i = i + 1;
        }
    }

    public(package) macro fun do($stop: _, $f: |_|) {
        range_do!(0, $stop, $f)
    }

    public(package) macro fun do_eq($stop: _, $f: |_|) {
        range_do_eq!(0, $stop, $f)
    }
}
