module A::m {
    #[deprecated(note = b"use a different struct instead")]
    public struct Bar() has drop;


    public fun make_bar(): Bar {
        Bar()
    }


    #[deprecated(note = b"use a different function instead")]
    public fun deprecated_function() { }
}
