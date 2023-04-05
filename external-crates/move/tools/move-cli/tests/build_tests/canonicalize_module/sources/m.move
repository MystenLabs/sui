module foo::a {
    public fun f(): u64 {
        42
    }
}

module bar::b {
    public fun f(): u64 {
        43
    }
}

module bar::c {
    struct B { x: u64 }
    struct A { b: vector<B> }

    public fun g(): u64 {
        foo::a::f() +
        bar::b::f() +
        qux::e::g(qux::e::b())
    }

    public fun f(): u64 {
        baz::d::f() +
        qux::e::f(qux::e::a())
    }
}

module baz::d {
    public fun f(): u64 {
        45
    }
}

module qux::e {
    struct B has drop { x: u64 }
    struct A has drop { x: u64 }

    public fun a(): A {
        A { x: 46 }
    }

    public fun b(): B {
        B { x: 47 }
    }

    public fun f(a: A): u64 {
        a.x
    }

    public fun g(b: B): u64 {
        b.x
    }
}
