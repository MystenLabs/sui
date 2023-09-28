// tests the helper when a dot call fails to resolve
module a::space {
    public struct Point has copy, drop {
        x: u64,
        y: u64,
    }

    public struct Line has copy, drop {
        a: Point,
        b: Point,
    }

    public fun zero(): Point {
        Point { x: 0, y: 0 }
    }

    public fun vec(p: &Point): Line {
        Line { a: zero(),  b: *p, }
    }

    public fun len(_: &Line): u64 {
        abort 0
    }
}


module a::example {
    use a::space::Point;

    #[allow(unused)]
    public fun t(p: &Point) {
        // TODO would be nice to help with this one
        use a::space::{zero as z, len as l};
        p.z();
        p.l();
    }
}
