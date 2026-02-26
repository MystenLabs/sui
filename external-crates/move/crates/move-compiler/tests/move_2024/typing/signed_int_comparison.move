// tests comparison operations on signed types
module a::m {
    fun cmp() {
        let a: i64 = 10i64;
        let b: i64 = 5i64;
        let _lt = a < b;
        let _gt = a > b;
        let _le = a <= b;
        let _ge = a >= b;
        let _eq = a == b;
        let _ne = a != b;
    }
}
