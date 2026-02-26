// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x42::m {
    struct Point has copy, drop {
        x: i32,
        y: i32,
    }

    struct Rect has copy, drop {
        top_left: Point,
        bottom_right: Point,
    }

    fun create_point(): Point {
        Point { x: -10i32, y: 20i32 }
    }

    fun create_rect(): Rect {
        Rect {
            top_left: Point { x: 0i32, y: 100i32 },
            bottom_right: Point { x: 100i32, y: 0i32 },
        }
    }

    fun access_fields() {
        let p = create_point();
        let _x: i32 = p.x;
        let _y: i32 = p.y;
    }

    fun modify_field() {
        let mut p = Point { x: 1i32, y: 2i32 };
        p.x = -p.x;
        p.y = p.y + 1i32;
    }

    fun destructure() {
        let Point { x, y } = create_point();
        let _sum: i32 = x + y;
    }
}
