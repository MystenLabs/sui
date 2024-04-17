module 0x42::m {

    public enum ABC<T> {
        A(T),
        B,
        C(T)
    }

    fun t0(): u64 {
        match (ABC::C(0)) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => 1,
        }
    }

}
