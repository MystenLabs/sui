// tests signed integers as function parameters and return types
module a::m {
    fun add_signed(a: i64, b: i64): i64 {
        a + b
    }

    fun call_it() {
        let _r = add_signed(1i64, 2i64);
    }
}
