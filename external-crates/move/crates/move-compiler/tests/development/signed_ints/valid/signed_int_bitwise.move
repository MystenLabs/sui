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

    fun bitwise_i256() {
        let a: i256 = 10i256;
        let b: i256 = 5i256;
        let _and = a & b;
        let _or = a | b;
        let _xor = a ^ b;
        let _shl = a << 2u8;
        let _shr = a >> 2u8;
    }
}
