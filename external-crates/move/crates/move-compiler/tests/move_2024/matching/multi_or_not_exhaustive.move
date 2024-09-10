module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B(T, T),
        C(T, T, T),
        D(T, T, T, T),
    }

    fun t0(): u64 {
        let subject = ABC::A(0);
        match (subject) {
            ABC::C(x, 5, _) | ABC::B(5, x) => x,
            ABC::B(x, 5) | ABC::A(x) | ABC::D(5, _, x, _) => x,
        }
    }

}
