// 'let ... else' against a `&T` subject auto-borrows the pattern: binders
// inside the pattern bind to references into the subject.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun by_ref(subject: &ABC<u64>): &u64 {
        let ABC::C(x) = subject else { abort 0 };
        x
    }

}
