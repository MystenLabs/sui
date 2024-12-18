// tests lints against combinable comparisons that can be simplified to a single comparison.
module a::m {
    fun t(x: u64, y: u64) {
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
        x != y && x == y;
        x > y && x == y;
        x < y && x == y;
        x >= y && x == y;
        x <= y && x == y;
        x > y && x != y;
        x < y && x != y;
        x >= y && x != y;
        x <= y && x != y;
        x < y && x > y;
        x >= y && x > y;
        x <= y && x > y;
        x >= y && x < y;
        x <= y && x < y;
        x <= y && x >= y;
        x != y || x == y;
        x > y || x == y;
        x < y || x == y;
        x >= y || x == y;
        x <= y || x == y;
        x > y || x != y;
        x < y || x != y;
        x >= y || x != y;
        x <= y || x != y;
        x < y || x > y;
        x >= y || x > y;
        x <= y || x > y;
        x >= y || x < y;
        x <= y || x < y;
        x <= y || x >= y;
    }

    fun flipped(x: u64, y: u64) {
        x == y && y != x;
        x == y && y > x;
        x == y && y < x;
        x == y && y >= x;
        x == y && y <= x;
        x != y && y > x;
        x != y && y < x;
        x != y && y >= x;
        x != y && y <= x;
        x > y && y > x;
        x > y && y >= x;
        x > y && y <= x;
        x < y && y >= x;
        x < y && y <= x;
        x >= y && y >= x;
        x == y || y != x;
        x == y || y > x;
        x == y || y < x;
        x == y || y >= x;
        x == y || y <= x;
        x != y || y > x;
        x != y || y < x;
        x != y || y >= x;
        x != y || y <= x;
        x > y || y > x;
        x > y || y >= x;
        x > y || y <= x;
        x < y || y >= x;
        x < y || y <= x;
        x >= y || y >= x;
        x != y && y == x;
        x > y && y == x;
        x < y && y == x;
        x >= y && y == x;
        x <= y && y == x;
        x > y && y != x;
        x < y && y != x;
        x >= y && y != x;
        x <= y && y != x;
        x < y && y < x;
        x >= y && y > x;
        x <= y && y > x;
        x >= y && y < x;
        x <= y && y < x;
        x <= y && y <= x;
        x != y || y == x;
        x > y || y == x;
        x < y || y == x;
        x >= y || y == x;
        x <= y || y == x;
        x > y || y != x;
        x < y || y != x;
        x >= y || y != x;
        x <= y || y != x;
        x < y || y < x;
        x >= y || y > x;
        x <= y || y > x;
        x >= y || y < x;
        x <= y || y < x;
        x <= y || y <= x;
    }

    fun same_op(x: u64, y: u64) {
        x == y && x == y;
        x != y && x != y;
        x > y && x > y;
        x < y && x < y;
        x >= y && x >= y;
        x <= y && x <= y;
        x == y || x == y;
        x != y || x != y;
        x > y || x > y;
        x < y || x < y;
        x >= y || x >= y;
        x <= y || x <= y;
        x == y && y == x;
        x != y && y != x;
        x > y && y < x;
        x < y && y > x;
        x >= y && y <= x;
        x <= y && y >= x;
    }

    const Values: bool = 5 > 3 || 5 == 3;

    fun values(): bool {
        5 != 3 && 5 > 3
    }

    #[allow(lint(redundant_ref_deref))]
    fun mismatched(x: u64, y: u64) {
        {&x} == &y || (x as u64) > (*&y:u64);
        &copy x == &y || x < move y;
    }
}
