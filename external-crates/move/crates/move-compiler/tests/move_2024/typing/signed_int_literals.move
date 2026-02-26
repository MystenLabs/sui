// tests signed integer literal syntax
module a::m {
    fun literals() {
        let _a: i8 = 1i8;
        let _b: i8 = 127i8;
        let _c: i16 = 100i16;
        let _d: i32 = 1000i32;
        let _e: i64 = 0i64;
        let _f: i128 = 42i128;
    }
}
