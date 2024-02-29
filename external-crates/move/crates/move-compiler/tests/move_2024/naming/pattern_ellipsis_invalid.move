module 0x42::m {
    public enum Y {
        D
    }

    public enum X {
        D()
    }

    public enum Z {
        D{}
    }

    public fun f(x: Y): u64 {
        match (x) {
            Y::D(..) => 0,
            Y::D{..} => 0,
            Y::D(x, ..) => 0,
            Y::D{x, ..} => 0,
        }
    }

    public fun g(x: X): u64 {
        match (x) {
            X::D{} => 0,
            X::D{..} => 0,
            X::D{x, ..} => 0,
        }
    }

    public fun h(x: Z): u64 {
        match (x) {
            Z::D() => 0,
            Z::D(..) => 0,
            Z::D(x, ..) => 0,
        }
    }
}
