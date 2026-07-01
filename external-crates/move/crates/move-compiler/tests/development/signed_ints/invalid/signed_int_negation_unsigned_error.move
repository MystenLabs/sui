// tests that unary minus on unsigned type should error
module a::m {
    fun neg_unsigned() {
        let a: u64 = 5;
        let _b = -a;
    }
}
