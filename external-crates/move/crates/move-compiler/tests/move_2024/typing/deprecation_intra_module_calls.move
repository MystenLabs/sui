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
