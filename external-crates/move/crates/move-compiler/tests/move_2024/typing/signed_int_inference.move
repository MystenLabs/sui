// tests type inference with signed integers
module a::m {
    fun inference() {
        let _x: i64 = 1;
        let _y: i8 = 5;
    }
}
