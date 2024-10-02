module a::m {
    struct S has copy, drop, store {}

    public use fun imm as S.f;

    fun val(_: S) {}

    fun imm(_: &S) {}

    fun mut(_: &mut S) {}

    fun nonono(s: S) {
        use fun mut as S.g;
        s.imm();
        s.f();
        s.mut();
        s.g();
        s.val();
    }
}
