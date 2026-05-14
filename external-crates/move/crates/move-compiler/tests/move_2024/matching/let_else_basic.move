module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun t0(): u64 {
        let subject = ABC::C(42u64);
        let ABC::C(x) = subject else { return 0 };
        x
    }

    fun t1(): u64 {
        let subject = ABC::A(10u64);
        let ABC::A(x) = subject else { abort 0 };
        x
    }

}
