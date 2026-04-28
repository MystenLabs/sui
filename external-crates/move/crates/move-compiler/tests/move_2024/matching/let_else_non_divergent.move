module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun non_divergent(): u64 {
        let subject = ABC::C(42u64);
        let ABC::C(x) = subject else { 0 };
        x
    }

}
