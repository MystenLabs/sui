module 0x42::m {
    public enum E<T> has drop {
        A(T),
        B,
        C(T, T),
    }
    fun t(): u64 {
        match (E::A(0)) {
            E::C(x, (0 | 1)) | E::B(x) | E::A(x) => x,
            _ => 1,
        }
    }
}
