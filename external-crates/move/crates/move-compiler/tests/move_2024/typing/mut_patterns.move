module 0x42::m {
    public enum X {
        A(u64, bool),
        B { x: u64 },
    }

    public fun good(x: X): u64 {
        match (x) {
            X::A(mut x, _) | X::B { mut x } => {
                x = x + 1;
                x
            },
        }
    }

    public fun good_mut_ref(x: &mut X) {
        match (x) {
            X::A(x, _) | X::B { x } => {
                *x = *x + 1;
            },
        }
    }

    public fun bad(x: X): u64 {
        match (x) {
            X::A(x, _) | X::B { x } => {
                x = x + 1;
                x
            },
        }
    }
}
