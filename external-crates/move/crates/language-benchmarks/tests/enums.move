// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x2::bench {
    const COUNT: u64 = 10_000;

    public enum Color has copy, drop {
        Red,
        Green,
        Blue,
    }

    public enum Shape has copy, drop {
        Circle { radius: u64 },
        Rectangle { width: u64, height: u64 },
        Triangle { base: u64, height: u64, side: u64 },
    }

    public enum Tree has copy, drop {
        Leaf { value: u64 },
        Node { left: u64, right: u64, data: u64 },
    }

    public fun bench_enum_create_unit() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _c = if (i % 3 == 0) {
                Color::Red
            } else if (i % 3 == 1) {
                Color::Green
            } else {
                Color::Blue
            };
            i = i + 1;
        }
    }

    public fun bench_enum_create_data() {
        let mut i: u64 = 0;
        while (i < COUNT) {
            let _s = if (i % 3 == 0) {
                Shape::Circle { radius: i }
            } else if (i % 3 == 1) {
                Shape::Rectangle { width: i, height: i + 1 }
            } else {
                Shape::Triangle { base: i, height: i + 1, side: i + 2 }
            };
            i = i + 1;
        }
    }

    public fun bench_enum_match_unit() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let c = if (i % 3 == 0) { Color::Red }
                    else if (i % 3 == 1) { Color::Green }
                    else { Color::Blue };
            acc = acc + match (c) {
                Color::Red => 1,
                Color::Green => 2,
                Color::Blue => 3,
            };
            i = i + 1;
        }
    }

    public fun bench_enum_match_data() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let s = if (i % 3 == 0) {
                Shape::Circle { radius: i }
            } else if (i % 3 == 1) {
                Shape::Rectangle { width: i, height: i + 1 }
            } else {
                Shape::Triangle { base: i, height: i + 1, side: i + 2 }
            };
            acc = acc + area(s);
            i = i + 1;
        }
    }

    public fun bench_enum_match_nested() {
        let mut i: u64 = 0;
        let mut acc: u64 = 0;
        while (i < COUNT) {
            let t = if (i % 2 == 0) {
                Tree::Leaf { value: i }
            } else {
                Tree::Node { left: i, right: i + 1, data: i + 2 }
            };
            acc = acc + match (t) {
                Tree::Leaf { value } => value,
                Tree::Node { left, right, data } => left + right + data,
            };
            i = i + 1;
        }
    }

    fun area(s: Shape): u64 {
        match (s) {
            Shape::Circle { radius } => radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height, side: _ } => base * height / 2,
        }
    }
}
