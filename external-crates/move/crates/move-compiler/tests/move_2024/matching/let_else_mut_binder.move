// 'let ... else' should support 'mut' binders inside the pattern, allowing
// the bound variable to be reassigned afterwards.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun mut_binder(): u64 {
        let subject = ABC::C(10u64);
        let ABC::C(mut x) = subject else { abort 0 };
        x = x + 1;
        x
    }

}
