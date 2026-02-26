// tests bitwise operations on signed types
module a::m {
    fun bitwise() {
        let a: i64 = 10i64;
        let b: i64 = 5i64;
        let _and = a & b;
        let _or = a | b;
        let _xor = a ^ b;
        let _shl = a << 2u8;
        let _shr = a >> 2u8;
    }
}
