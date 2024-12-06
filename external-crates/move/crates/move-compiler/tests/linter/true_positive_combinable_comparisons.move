module a::m {
    fun t(x: u64, y: u64): bool {
        x == y || x > y;
        x == y && x != y;
        x == y && x > y;
        x == y && x < y;
        x == y && x >= y;
        x == y && x <= y;
        x != y && x > y;
        x != y && x < y;
        x != y && x >= y;
        x != y && x <= y;
        x > y && x < y;
        x > y && x >= y;
        x > y && x <= y;
        x < y && x >= y;
        x < y && x <= y;
        x >= y && x <= y;
        x == y || x != y;
        x == y || x > y;
        x == y || x < y;
        x == y || x >= y;
        x == y || x <= y;
        x != y || x > y;
        x != y || x < y;
        x != y || x >= y;
        x != y || x <= y;
        x > y || x < y;
        x > y || x >= y;
        x > y || x <= y;
        x < y || x >= y;
        x < y || x <= y;
        x >= y || x <= y;
    }

    // fun flipped(x: u64, y: u64): bool {
    //     x == y && y != x;
    //     x == y && y > x;
    //     x == y && y < x;
    //     x == y && y >= x;
    //     x == y && y <= x;
    //     x != y && y > x;
    //     x != y && y < x;
    //     x != y && y >= x;
    //     x != y && y <= x;
    //     x > y && y < x;
    //     x > y && y >= x;
    //     x > y && y <= x;
    //     x < y && y >= x;
    //     x < y && y <= x;
    //     x >= y && y <= x;
    //     x == y || y != x;
    //     x == y || y > x;
    //     x == y || y < x;
    //     x == y || y >= x;
    //     x == y || y <= x;
    //     x != y || y > x;
    //     x != y || y < x;
    //     x != y || y >= x;
    //     x != y || y <= x;
    //     x > y || y < x;
    //     x > y || y >= x;
    //     x > y || y <= x;
    //     x < y || y >= x;
    //     x < y || y <= x;
    //     x >= y || y <= x;
    // }

    // const Values: bool = 5 > 3 || 5 == 3;

    // fun values(x: u64, y: u64): bool {
    //     5 != 3 && 5 > 3
    // }

    // fun refs(x: u64, y: u64): bool {
    //     &x == &y || x > y;
    // }
}
