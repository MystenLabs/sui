// 'let ... else' must work inside a macro body, surviving the macro
// recoloring/expansion machinery and binding callsite-local variables.
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    macro fun unwrap_c($subj: ABC<u64>): u64 {
        let ABC::C(x) = $subj else { abort 0 };
        x
    }

    fun caller(): u64 {
        unwrap_c!(ABC::C(7u64))
    }

}
