// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::m;

public struct Point(u64, u64) has copy, drop, store;
public struct FPoint { x: u64, y: u64 } has copy, drop, store;

macro fun foo(
    $x: &u64,
    $p: Point,
    $s: &mut FPoint,
    $f: |u64, (&u64, Point, &mut FPoint)|
) {
    $f(0, ($x, $p, $s))
}

fun t() {
    let p = Point(1, 2);
    foo!(
        &0,
        p,
        &mut FPoint { x: 3, y: 4 },
        |_, (_x, Point(mut xp, mut yp), FPoint { x, y })|  {
            assert!(xp == 1, 0);
            assert!(yp == 2, 0);
            xp = *x;
            yp = *y;
            *x = yp;
            *y = xp;
        }
    )
}
