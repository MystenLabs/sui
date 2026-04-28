module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun by_mut_ref(subject: &mut ABC<u64>): &mut u64 {
        let ABC::C(x) = subject else { abort 0 };
        x
    }

}
