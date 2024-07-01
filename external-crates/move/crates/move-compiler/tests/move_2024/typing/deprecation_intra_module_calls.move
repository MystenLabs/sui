#[deprecated(note = b"Use the k module instead.")]
module 0x7::l {
    public struct X() has drop;

    #[deprecated(note = b"Use the other struct instead.")]
    public struct Y() has drop;

    public struct Z(Y) has drop;

    public fun foo() { }

    public fun bar(_: &X) { }

    #[deprecated(note = b"Use the other function instead.")]
    public fun other(y: &Y): &Y { y }

    public fun internal_calller() {
        // Should not give us a deprecation warning since it's an internal caller of a deprecated module.
        foo();
        // Should give us a deprecated warning since it's an internal caller of a deprecated function.
        internal();
        // Should give a warning for `other` and `Y` separately since they reference different deprecations
        let y = other(&Y());

        // This should be grouped in with the other calls to `other` in this function though
        other(other(other(other(other(other(other(other(other(other(y))))))))));
    }

    #[deprecated(note = b"This is a deprecated function within a deprecated module.")]
    fun internal() { }
}
