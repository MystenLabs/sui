// Loading of types and functions across packages that have a shared dependency package.
module 0x1::a {
    public struct X()
    public enum Y { A } 

    public fun f() { }
}

module 0x1::b {
    public struct X(0x1::a::X)
    public enum Y<T> { A(T) }
    public fun g() { 
        0x1::a::f();
    }

}

module 0x2::c {
    public struct X()

    public struct L { 
        x0: X,
        x1: 0x1::b::X,
        x2: 0x1::a::X,
    }

    public enum E {
        A,
        B(0x1::b::Y<0x1::a::X>),
        C(0x1::b::Y<X>),
    }
    public fun h() { 0x1::b::g(); }
}

module 0x3::c {
    public struct X()

    public struct L { 
        x0: X,
        x1: 0x1::b::X,
        x2: 0x1::a::X,
    }

    public enum E {
        A,
        B(0x1::b::Y<0x1::a::X>),
        C(0x1::b::Y<X>),
        D(0x2::c::X),
    }
    public fun h() { 
        0x1::b::g(); 
        0x2::c::h();
    }
}

