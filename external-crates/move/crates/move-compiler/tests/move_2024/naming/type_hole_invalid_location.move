module a::m {
    public struct P(_)
    public struct S { f: _ }
    public enum E { P(_), S { f: _ } }
    const C: _ = 0;
    fun foo(_: _) {}
    fun bar(): _ { 0 }
}
