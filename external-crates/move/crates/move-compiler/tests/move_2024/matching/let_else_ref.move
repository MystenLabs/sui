// let-else does not currently support borrow patterns on the LHS.
// The pattern is parsed as a bind and converted, which produces a value pattern.
// This test verifies the error message when the subject is a reference.
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
