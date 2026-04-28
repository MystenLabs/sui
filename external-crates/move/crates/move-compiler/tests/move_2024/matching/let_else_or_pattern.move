module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun or_pattern(): u64 {
        let subject = ABC::C(42u64);
        let ABC::C(x) | ABC::A(x) = subject else { return 0 };
        x
    }

}
