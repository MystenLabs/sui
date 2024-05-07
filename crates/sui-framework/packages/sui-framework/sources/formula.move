// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[allow(unused_field)]
module sui::formula {
    use sui::math;

    const EOverflow: u64 = 0;
    const EUnderflow: u64 = 1;
    const EDivideByZero: u64 = 2;

    public struct Expr<T> has copy, drop {
        op: vector<u8>,
        args: vector<T>,
    }

    public struct Formula<T> has copy, drop {
        expressions: vector<Expr<T>>,
        scaling: Option<T>
    }

    public fun new<T>(): Formula<T> {
        Formula { expressions: vector[], scaling: option::none() }
    }

    public fun div<T>(mut self: Formula<T>, other: T): Formula<T> {
        self.expressions.push_back(Expr { op: b"div", args: vector[ other ] });
        self
    }

    public fun mul<T>(mut self: Formula<T>, other: T): Formula<T> {
        self.expressions.push_back(Expr { op: b"mul", args: vector[ other ] });
        self
    }

    public fun add<T>(mut self: Formula<T>, other: T): Formula<T> {
        self.expressions.push_back(Expr { op: b"add", args: vector[ other ] });
        self
    }

    public fun sub<T>(mut self: Formula<T>, other: T): Formula<T> {
        self.expressions.push_back(Expr { op: b"sub", args: vector[ other ] });
        self
    }

    // public fun integrated<T>(mut self: Formula<T>, other: Formula<T>): Formula<T> {
    //     self.expressions.append(other.expressions);
    //     self
    // }

    public fun scale<T>(mut self: Formula<T>, scaling: T): Formula<T> {
        self.scaling.fill(scaling);
        self
    }

    public fun sqrt<T>(mut self: Formula<T>): Formula<T> {
        self.expressions.push_back(Expr { op: b"sqrt", args: vector[] });
        self
    }

    public fun calculate_u8(self: Formula<u8>, value: u8): u8 {
        let Formula { mut expressions, scaling: _ } = self;
        let mut result = value as u16;
        expressions.reverse();
        while (expressions.length() > 0) {
            let Expr { op, args } = expressions.pop_back();
            if (op == b"div") {
                result = result / (args[0] as u16);
            } else if (op == b"mul") {
                result = result * (args[0] as u16);
            } else if (op == b"add") {
                result = result + (args[0] as u16);
            } else if (op == b"sub") {
                result = result - (args[0] as u16);
            } else if (op == b"sqrt") {
                result = math::sqrt((result as u64) * 10000) as u16;
            }
        };

        assert!(result < 255, EOverflow);
        (result as u8)
    }

    public fun calculate_u64(self: Formula<u64>, value: u64): u64 {
        let Formula { mut expressions, scaling: _ } = self;
        let mut result = value;
        expressions.reverse();
        while (expressions.length() > 0) {
            let Expr { op, args } = expressions.pop_back();
            if (op == b"div") {
                result = result / args[0];
            } else if (op == b"mul") {
                result = result * args[0];
            } else if (op == b"add") {
                result = result + args[0];
            } else if (op == b"sub") {
                result = result - args[0];
            } else if (op == b"sqrt") {
                result = math::sqrt(result * 10000);
            }
        };

        result
    }

    public fun calculate_u128(self: Formula<u128>, value: u128): u128 {
        let Formula { mut expressions, scaling } = self;
        let scaling = scaling.destroy_with_default(1 << 64) as u256;
        let mut is_scaled = false;
        let mut result = (value as u256);

        expressions.reverse();

        while (expressions.length() > 0) {
            let Expr { op, args } = expressions.pop_back();
            if (op == b"div") {
                assert!(args[0] != 0, EDivideByZero);
                if (is_scaled) {
                    result = (result) / (args[0] as u256);
                } else {
                    result = (result * scaling) / (args[0] as u256);
                    is_scaled = true;
                }
            } else if (op == b"mul") {
                result = result * (args[0] as u256);
            } else if (op == b"add") {
                if (is_scaled) {
                    result = result + (args[0] as u256 * scaling);
                } else {
                    result = result + (args[0] as u256);
                }
            } else if (op == b"sub") {
                if (is_scaled) {
                    assert!(result >= (args[0] as u256 * scaling), EUnderflow);
                    result = result - (args[0] as u256 * scaling);
                } else {
                    assert!(result >= (args[0] as u256), EUnderflow);
                    result = result - (args[0] as u256);
                }
            } else if (op == b"sqrt") {
                if (is_scaled) {
                    result = sqrt_u256(result * scaling);
                } else {
                    result = sqrt_u256(result * scaling * scaling);
                    is_scaled = true;
                }
            }
        };

        if (is_scaled) {
            result = result / scaling;
        };

        assert!(result < 340_282_366_920_938_463_463_374_607_431_768_211_455u256, EOverflow);

        result as u128
    }

    // public fun calculate_u16(self: Formula<u16>, value: u16): u16 { /* ... */ 100 }
    // public fun calculate_u32(self: Formula<u32>, value: u32): u32 { /* ... */ 100 }
    // public fun calculate_u64(self: Formula<u64>, value: u64): u64 { /* ... */ 100 }

    #[test] fun test_formula() {

        let form = new()
            .add(10u8)
            .mul(100)
            .div(10)
            .sub(5);

        assert!((*&form).calculate_u8(5) == 145, 0);
        assert!((*&form).calculate_u8(10) == 195, 0);

        let formula = new<u128>()
            .scale(1 << 64)
            .div(10000)
            .add(1)
            .sqrt()
            .mul(412481737123559485879);

        let res = formula.calculate_u128(100);
        let test_scaling = new().div(1).div(1).div(1).div(1).calculate_u128(1);

        assert!(test_scaling == 1, 0);

        // 414539015407565617054 (expected result)
        // 414539015407565617051

        // 414539015361330940475


        std::debug::print(&res);




        // let mut form = new();
        // form.sqrt();
        // .integrated(form)
    }


    // === Polyfill ===

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


    public fun min_u256(x: u256, y: u256): u256 {
        if (x < y) {
            x
        } else {
            y
        }
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
