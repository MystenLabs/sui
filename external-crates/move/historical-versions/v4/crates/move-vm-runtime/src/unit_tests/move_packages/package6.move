module 0x1::a {
    public struct AA() has drop;

    public fun f<T>(x: T): T  { x }
}

module 0x2::b {
    public struct BB() has drop;

    public fun g<T>(x: T): T  { 0x1::a::f(x) }

    public fun h() { 0x1::a::f(BB()); }

    public fun i(x: 0x1::a::AA) { 0x1::a::f(x); }
}
