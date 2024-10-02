module 0x42::m {
    public enum R has drop {
        Bad(A),
    }

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
        foo();
        bar_dep();
    }

    public fun qux(_: A) { }

    public fun return_dep(): A {
        A()
    }

    public fun use_const(): u8 { 
        H
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
        let _ = R::Bad(A());
    }
}
