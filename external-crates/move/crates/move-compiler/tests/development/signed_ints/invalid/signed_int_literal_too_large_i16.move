module a::m {
    fun bad() {
        32768i16;
        let _x: i16 = 40000i16;
        -32769i16;
    }
}
