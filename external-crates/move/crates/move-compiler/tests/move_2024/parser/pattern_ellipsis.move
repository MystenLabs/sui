module 0x42::m {
    public enum X has drop {
        A { x: u64 },
        B { x: u64, y: u64 },
        C(u64, bool, bool),
    }

    public fun f(x: X): u64 {
        match (x) {
            X::A { .. } => 0,
            X::B { x, .. } => x,
            X::C(1, ..) => 1,
            X::C(1, .., true) => 2,
            X::C(.., true) => 1,
            X::C(..) => 1,
        }
    }
}
