module a::m {

    fun ops(x: u8, y: u8) {
        x + y as u32;
        x - y as u32;
        x * y as u32;
        x / y as u32;
        x % y as u32;
        x & y as u32;
        x | y as u32;
        x ^ y as u32;
        x << y as u32;
        x >> y as u32;
        !x as u32;
    }

}
