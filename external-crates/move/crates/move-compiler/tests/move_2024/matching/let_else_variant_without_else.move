module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun bad(subject: ABC<u64>): u64 {
        let ABC::C(x) = subject;
        x
    }

}
