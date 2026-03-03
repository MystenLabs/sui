// tests unary minus on signed types
module a::m {
    fun negation() {
        let a: i64 = 5i64;
        let _b: i64 = -a;
        let _c: i8 = -(1i8);
    }
}
