module 0x42::m {
    public enum X {
        A(u64),
        B(u64, bool),
    }

    public fun f(x: X): u64 {
        match (x) {
            X::A(mut x) | X::B(_, _) => {
                x = x + 1;
                x
            }
        }
    }

    public fun g(x: X): u64 {
        match (x) {
            X::A(_) | X::B(mut x, _) => {
                x = x + 1;
                x
            }
        }
    }
}
