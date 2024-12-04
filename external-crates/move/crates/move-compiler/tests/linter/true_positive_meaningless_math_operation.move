module 0x42::M {
    public fun unchanged(x: u64) {
        x * 1;
        1 * x;
        x / 1;
        x + 0;
        0 + x;
        x - 0;
        x << 0;
        x << 0;
    }

    public fun always_zero(x: u64) {
        x * 0;
        0 * x;
        0 / x;
        0 % x;
        x % 1;
    }

    public fun always_one(x: u64) {
        1 % x;
    }

    public fun ast_fold(x: u64) {
        let y = 1;
        x * y;
        x - (1 - 1);
    }

    public fun t_u8(x: u8) {
        x * 1;
        x * 0;
        1 % x;
    }

    public fun t_u16(x: u16) {
        x * 1;
        x * 0;
        1 % x;
    }

    public fun t_u32(x: u32) {
        x * 1;
        x * 0;
        1 % x;
    }

    public fun t_u128(x: u128) {
        x * 1;
        x * 0;
        1 % x;
    }

    public fun t_u256(x: u256) {
        x * 1;
        x * 0;
        1 % x;
    }
}
