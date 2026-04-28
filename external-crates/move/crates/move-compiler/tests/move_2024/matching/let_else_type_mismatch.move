module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public enum Other has drop {
        X(u64),
    }

    fun wrong_type(): u64 {
        let subject = ABC::C(42u64);
        let Other::X(x) = subject else { return 0 };
        x
    }

}
