module 0x42::Test {
    fun foo(cond: bool) {
        // NOTE: `foo` will not be treated as a pure function because in
        // move-model, the only thing allowed in an exp block is let-bindings
        // (except for the last expression). This will cause the type unifier
        // to treat the `addr` to have a type which is the same as the `dummy()`
        // function call return type. This is clearly an erroneous state, but is
        // okay because we have previously marked the spec fucntion translation
        // as failure and subsequent errors can be ignored.
        let addr = if (cond) {
            dummy(0);
            @0x1
        } else {
            dummy(42);
            @0x2
        };
        bar(addr);
    }

    fun bar(_addr: address) {}

    fun dummy(val: u8): u8 { val }
}
