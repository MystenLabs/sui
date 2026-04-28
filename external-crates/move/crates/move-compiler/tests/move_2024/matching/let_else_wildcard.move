module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun wildcard_inner(): u64 {
        let subject = ABC::C(5u64);
        let ABC::C(_) = subject else { return 0 };
        1
    }

}
