module 0x1::a {
    fun f() { 
        0x1::b::g(); 
        0x1::c::h();
    }
}

module 0x1::b {
    public fun g() { }

}

module 0x1::c {
    public fun h() { }
}
