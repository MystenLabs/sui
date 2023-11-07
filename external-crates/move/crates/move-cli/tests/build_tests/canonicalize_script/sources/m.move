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

script {
    fun main() {
        let a = qux::d::a();
        let b = qux::d::b();

        assert!(
            foo::a::f() +
            bar::b::f() +
            baz::c::f() +
            qux::d::g(b) +
            qux::d::f(a) ==
            42 + 43 + 44 + 45 + 46,
            0,
        );
    }
}

module baz::c {
    public fun f(): u64 {
        44
    }
}

module qux::d {
    struct B has drop { x: u64 }
    struct A has drop { x: u64 }

    public fun a(): A {
        A { x: 45 }
    }

    public fun b(): B {
        B { x: 46 }
    }

    public fun f(a: A): u64 {
        a.x
    }

    public fun g(b: B): u64 {
        b.x
    }
}
