// tests arithmetic operations on signed types
module a::m {
    fun arith() {
        let a: i64 = 10i64;
        let b: i64 = 5i64;
        let _add = a + b;
        let _sub = a - b;
        let _mul = a * b;
        let _div = a / b;
        let _mod = a % b;
    }

    fun arith_i256() {
        let a: i256 = 10i256;
        let b: i256 = 5i256;
        let _add = a + b;
        let _sub = a - b;
        let _mul = a * b;
        let _div = a / b;
        let _mod = a % b;
    }
}
