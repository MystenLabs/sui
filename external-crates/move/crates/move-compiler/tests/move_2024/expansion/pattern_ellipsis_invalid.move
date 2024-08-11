module 0x42::m {
    public enum X has drop {
        A { x: u64 },
        B { x: u64, y: u64 },
        C(u64, bool, bool),
    }

    public fun g(x: X): u64 {
        match (x) {
            X::A { .., .. } => 0,
            X::B { .., .., .. } => 0,
            X::B { .., .., x} => x,
            X::B { .., x, .. } => x,
            X::C(.., x, ..) => 1,
            X::C(.., ..) => 1,
        }
    }
}
