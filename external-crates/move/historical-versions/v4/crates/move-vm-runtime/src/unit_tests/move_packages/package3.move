module 0x1::a {
    public fun f() { 0x1::b::g(); }
}

module 0x1::b {
    public fun g() { }
}

module 0x2::c {
    public fun g() { 0x1::a::f(); }
}
