// tests casting between signed types and between signed/unsigned
module a::m {
    fun casting() {
        let a: i8 = 10i8;
        let _b: i16 = a as i16;
        let _c: i32 = a as i32;
        let _d: i64 = a as i64;
        let _e: i128 = a as i128;
    }
}
