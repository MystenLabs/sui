#[deprecated(note = b"Use the k module instead.")]
module 0x7::l {
    public struct X() has drop;

    public fun foo() { }

    public fun bar(_: &X) { }

    #[deprecated(note = b"Use the other function instead.")]
    public fun other(_: &X) { }

    public fun internal_calller() {
        // Should not give us a deprecation warning since it's an internal caller of a deprecated module.
        foo();
        // Should give us a deprecated warning since it's an internal caller of a deprecated function.
        internal();
    }

    #[deprecated(note = b"This is a deprecated function within a deprecated module.")]
    fun internal() { }

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


    public enum R has drop {
        Bad(A),
    }

    use fun 0x7::l::other as X.lol;

    public struct B() has drop;

    #[deprecated(note = b"Use the B struct instead.")]
    public struct A() has drop;

    #[deprecated]
    public fun foo() { }

    #[deprecated(note = b"Use the baz function instead.")]
    public fun bar_dep() { }

    #[deprecated(note = b"Use the L constant instead.")]
    const H: u8 = 0x42;

    #[error, deprecated(note = b"Use `NewError` instead.")]
    const OldError: vector<u8> = b"old error";

    const NewError: vector<u8> = b"new error";

    public fun baz() { 
        foo();
        bar_dep();
    }

    public fun qux(_: A) { }

    public fun lol(_: &X) { }

    public fun quux(x: 0x7::l::X) {
        0x7::l::foo();
        x.bar();
    }

    public fun quux_use() { 
        use 0x7::l;

        l::foo(); 
    }

    public fun use_const(): u8 { 
        H
    }

    public fun bar(_: &B) { }

    public fun dep_meth_call(x: &X) {
        x.bar();

        // We should give deprecation notices through use fun declarations.
        // Note that we get a function deprecated warning here, not a module deprecated warning.
        // This is because we favor more specific deprecations over more general ones.
        x.lol();

        // No deprecation warning since it's a method call to a non-deprecated function post-resolution.
        B().bar();
    }

    public fun clever_error_deprecated() {
        abort OldError
    }

    public fun matcher() {
        let y = (A(): A);
        match (y) {
            A() => (),
        };
        A() = A();
        let A() = A();
    }

    public fun dep_enum() {
        let _ = T::Some(0x7::l::make_x());
        let _ = T::Other { y: 0x7::l::make_x()};
        let _ = T::Fine;
        let _ = R::Bad(A());
    }
}
