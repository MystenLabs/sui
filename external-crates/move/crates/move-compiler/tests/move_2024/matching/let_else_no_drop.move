module 0x42::m {

    public enum ABC<T> {
        A(T),
        B,
        C(T)
    }

    fun no_drop(): u64 {
        let subject = ABC::C(42u64);
        let ABC::C(x) = subject else { return 0 };
        x
    }

}
