#[deprecated(note = b"This module is deprecated")]
module A::mod_deprecated {
    public struct F() has drop;

    public fun make_f(): F {
        F()
    }

    #[deprecated(note = b"This function is deprecated with a deprecated module")]
    public fun deprecated_function() { }
}

