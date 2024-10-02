module a::m {
    macro const X: u64 = 0;
    macro public struct S()
    macro use a::m as n;
    macro use fun foo as S.bar;
    fun foo(s: S): S { s }
}
