module a::m {
    public(friend) use fun foo as S.g;
    public(package) use fun foo as S.h;
    public struct S {}
    fun foo(s: &S) {
        s.g();
        s.h();
        abort 0
    }
}
