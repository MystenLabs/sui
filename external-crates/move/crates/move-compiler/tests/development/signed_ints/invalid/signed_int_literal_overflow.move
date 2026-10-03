// tests literal overflow for signed types
module a::m {
    fun overflow() {
        let _a: i8 = 128i8;
    }
}
