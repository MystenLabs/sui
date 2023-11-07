// tests that control structures are right associative when not immediately followed by a block

// valid usage with field access

module 0x42::M {

    struct S has copy, drop { f: u64 }

    fun t(cond: bool, s1: S, s2: S) {
        let _: u64 = if (cond) 0 else s2.f;
        let _: u64 = if (cond) s1.f else s2.f;
        let _: u64 = if (cond) s1 else { s2 }.f;
        let _: u64 = if (cond) { s1 } else { s2 }.f;
    }

}
