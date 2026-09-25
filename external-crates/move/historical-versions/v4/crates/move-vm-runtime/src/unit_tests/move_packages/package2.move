module 0x1::a {
    public struct X()
    public enum Y { A } 

    fun f() { 
        0x1::b::g(); 
        0x1::c::h();
    }
}

module 0x1::b {
    public struct X()
    public enum Y { A }
    public fun g() { }

}

module 0x1::c {
    public fun h() { }
}
