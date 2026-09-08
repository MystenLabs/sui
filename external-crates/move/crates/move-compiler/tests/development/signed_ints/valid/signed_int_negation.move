// tests unary minus on signed types
module a::m {
    fun negation() {
        let a: i64 = 5i64;
        let _b: i64 = -a;
        let _c: i8 = -(1i8);
    }

    fun negation_i256() {
        let a: i256 = 5i256;
        let _b: i256 = -a;
        let _c: i256 = -(1i256);
    }
}
