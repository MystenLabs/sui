module 0x42::m {
    public enum E has drop {
        A(u64),
        C(u64, u64, u64),
    }

    fun t(): u64 {
        let subject = E::A(0);
        match (subject) {
            E::A(x) if (x == &0) => x,
            // Error: Invalid arg count for C, should be 3
            E::A(x) | E::C(x, _, 0) | E::C(_, x) => x,
        }
    }
}
