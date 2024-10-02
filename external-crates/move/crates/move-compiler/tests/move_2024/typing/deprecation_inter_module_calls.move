#[deprecated(note = b"Use the k module instead.")]
module 0x7::l {
    public struct X() has drop;

    public fun foo() { }

    public fun bar(_: &X) { }

    #[deprecated(note = b"More specific deprecations override less specific ones.")]
    public fun other_dep(_: &X) { }

    public fun other(_: &X) { }

    public fun make_x(): X {
        X()
    }
}

module 0x42::m {
    use 0x7::l::X;

    public struct Y has drop {
        f: X,
    }

    public enum T has drop {
        Some(X),
        Other { y: X },
        Fine,
    }

    public struct B() has drop;

    use fun 0x7::l::other_dep as X.lol;

    use fun 0x7::l::other as X.rofl;

    public fun lol(_: &X) { }

    public fun quux(x: 0x7::l::X) {
        0x7::l::foo();
        x.bar();
    }

    public fun quux_use() { 
        use 0x7::l;
        l::foo(); 
    }

    public fun bar(_: &B) { }

    public fun dep_meth_call(x: &X) {
        x.bar();

        // We should give deprecation notices through use fun declarations.
        // Note that we get a function deprecated warning here, not a module deprecated warning.
        // This is because we favor more specific deprecations over more general ones.
        x.lol();
        x.rofl();

        // No deprecation warning since it's a method call to a non-deprecated function post-resolution.
        B().bar();
    }

    public fun dep_enum() {
        let _ = T::Some(0x7::l::make_x());
        let _ = T::Other { y: 0x7::l::make_x()};
        let _ = T::Fine;
    }
}
