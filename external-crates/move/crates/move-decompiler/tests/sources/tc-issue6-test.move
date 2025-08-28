module 0x12::tc_i6 {
    fun calculate(x: u256, y:u256, z:u256): u256 {
        let y_prev = y;
        if (z > 1000) {
            y = y + 1;
        } else {
            y = y - 1;
        };
        if (y > y_prev) {
            return x
        } else {
            return y
        }
    }
}
