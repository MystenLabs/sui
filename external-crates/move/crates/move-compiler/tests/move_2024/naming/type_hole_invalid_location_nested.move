module a::m {
    public struct Pair<T1, T2>(T1, T2) has copy, drop, store;
    public struct P(Pair<u64, _>)
    public struct S { f: Pair<_, bool> }
    public enum E { P(Pair<_, _>), S { f: vector<_> } }
    const C: vector<_> = vector[0];
    fun foo(p: Pair<_, vector<u8>>): vector<u8> { p.1 }
    fun bar(): Pair<_, _> { Pair(any(), any()) }

    fun any<T>(): T { abort 0 }
}
