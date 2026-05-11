// 'let ... else' with a pattern that has no binders (e.g. matching a unit
// variant) is well-formed: the success branch falls through with no new
// locals, the else branch diverges.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun is_b(subject: ABC<u64>): bool {
        let ABC::B = subject else { return false };
        true
    }

}
