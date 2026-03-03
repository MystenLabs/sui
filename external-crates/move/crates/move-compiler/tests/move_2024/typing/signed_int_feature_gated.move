// tests that signed integer types are gated behind the development edition
module a::m {
    fun gated_type() {
        let _x: i8 = 0;
    }

    fun gated_negation() {
        let _x = -1;
    }
}
