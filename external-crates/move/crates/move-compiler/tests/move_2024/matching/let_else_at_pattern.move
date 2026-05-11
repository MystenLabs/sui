// '@' patterns must be permitted on the let-else LHS: the outer binder gets
// the whole value, the inner pattern still gates whether the success arm runs.
module 0x42::m {

    public enum ABC<T> has copy, drop {
        A(T),
        B,
        C(T)
    }

    fun keep_whole(subject: ABC<u64>): ABC<u64> {
        let v @ ABC::C(_) = subject else { abort 0 };
        v
    }

}
