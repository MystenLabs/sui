module 0x42::m {
    public enum E<T> has drop {
        A,
        C(T, T, T),
    }

    fun t(): u64 {
        // Void type in subject position type argument, so not an "error" in the type exactly.
        let subject = E::C({abort 0},{abort 0},{abort 0});
        match (subject) {
            // Error: extra arguments in pattern
            E::A(x) if (true) => x,
            // Error: extra arguments in pattern
            E::A(x) | E::C(x, _, 0) => x,
            _ => 1,
        }
    }
}
