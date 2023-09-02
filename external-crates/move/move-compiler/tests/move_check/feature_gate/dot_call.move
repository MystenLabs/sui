module a::m {
    struct S has copy, drop, store {}

    fun val(_: S) {}

    fun imm(_: &S) {}

    fun mut(_: &mut S) {}

    fun nonono(s: S) {
        s.imm();
        s.mut();
        s.val();
    }
}
