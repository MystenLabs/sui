module 0x42::m {
    public enum E<T> has drop {
        A,
        C(T, T, T),
    }

    fun t(): u64 {
        // Error: extra arguments in pattern
        let subject = E::A(0);
        match (subject) {
            // Error: extra arguments in pattern
            E::A(x) if (true) => x,
            // Error: extra arguments in pattern
            E::A(x) | E::C(x, _, 0) => x,
            _ => 1,
        }
    }
}
