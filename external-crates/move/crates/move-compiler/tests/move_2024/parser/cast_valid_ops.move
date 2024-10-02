module a::m {
    // in each case, the cast has higher precedence than the operator
    fun ops(x: u8, y: u32) {
        x == y as u8;
        x != y as u8;
        x < y as u8;
        x <= y as u8;
        x > y as u8;
        x >= y as u8;

        x as u32 == y;
        x as u32 != y;
        x as u32 < y;
        x as u32 <= y;
        x as u32 > y;
        x as u32 >= y;
    }
}
