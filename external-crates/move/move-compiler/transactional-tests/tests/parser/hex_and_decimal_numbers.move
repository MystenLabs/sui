//# run
script {
    fun main() {
        assert!(15u8 == 0xFu8, 42);
        assert!(15u8 == 0x0Fu8, 42);
        assert!(255u8 == 0xFFu8, 42);
        assert!(255u8 == 0x0FFu8, 42);

        assert!(15u16 == 0xFu16, 42);
        assert!(15u16 == 0x0Fu16, 42);
        assert!(255u16 == 0xFFu16, 42);
        assert!(255u16 == 0x0FFu16, 42);
        assert!(4095u16 == 0xFFFu16, 42);
        assert!(65535u16 == 0xFFFFu16, 42);
        assert!(65535u16 == 0x00FFFFu16, 42);


        assert!(15u32 == 0xFu32, 42);
        assert!(15u32 == 0x0Fu32, 42);
        assert!(255u32 == 0xFFu32, 42);
        assert!(255u32 == 0x0FFu32, 42);
        assert!(4095u32 == 0xFFFu32, 42);
        assert!(65535u32 == 0xFFFFu32, 42);
        assert!(4294967295u32 == 0xFFFFFFFFu32, 42);
        assert!(4294967295u32 == 0x00FFFFFFFFu32, 42);

        assert!(15u64 == 0xFu64, 42);
        assert!(15u64 == 0x0Fu64, 42);
        assert!(255u64 == 0xFFu64, 42);
        assert!(255u64 == 0x0FFu64, 42);
        assert!(18446744073709551615u64 == 0xFFFFFFFFFFFFFFFFu64, 42);
        assert!(18446744073709551615u64 == 0x0FFFFFFFFFFFFFFFFu64, 42);

        assert!(15u128 == 0xFu128, 42);
        assert!(15u128 == 0x0Fu128, 42);
        assert!(255u128 == 0xFFu128, 42);
        assert!(255u128 == 0x0FFu128, 42);
        assert!(18446744073709551615u128 == 0xFFFFFFFFFFFFFFFFu128, 42);
        assert!(18446744073709551615u128 == 0x0FFFFFFFFFFFFFFFFu128, 42);
        assert!(
            340282366920938463463374607431768211455u128 == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128,
            42,
        );
        assert!(
            340282366920938463463374607431768211455u128 == 0x0FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128,
            42,
        );

        assert!(15u128 == 0xFu128, 42);
        assert!(15u128 == 0x0Fu128, 42);
        assert!(255u128 == 0xFFu128, 42);
        assert!(255u128 == 0x0FFu128, 42);
        assert!(18446744073709551615u128 == 0xFFFFFFFFFFFFFFFFu128, 42);
        assert!(18446744073709551615u128 == 0x0FFFFFFFFFFFFFFFFu128, 42);
        assert!(
            340282366920938463463374607431768211455u128 == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128,
            42,
        );
        assert!(
            340282366920938463463374607431768211455u128 == 0x0FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu128,
            42,
        );

        assert!(15u256 == 0xFu256, 42);
        assert!(15u256 == 0x0Fu256, 42);
        assert!(255u256 == 0xFFu256, 42);
        assert!(255u256 == 0x0FFu256, 42);
        assert!(18446744073709551615u256 == 0xFFFFFFFFFFFFFFFFu256, 42);
        assert!(18446744073709551615u256 == 0x0FFFFFFFFFFFFFFFFu256, 42);
        assert!(
            340282366920938463463374607431768211455u256 == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu256,
            42,
        );
        assert!(
            115792089237316195423570985008687907853269984665640564039457584007913129639935u256 == 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu256,
            42,
        );
        assert!(
            115792089237316195423570985008687907853269984665640564039457584007913129639935u256 == 0x0FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu256,
            42,
        );
        assert!(
            115792089237316195423570985008687907853269984665640564039457584007913129639935u256 == 0x00FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFu256,
            42,
        );
    }
}
