module 0x2::A {
    #[test]
    public fun load_constant_number_literal_u8() {
        let a = 1_0u8;
        let b: u8 = 10;
        assert!(a == b, 1);
    }

    #[test]
    public fun load_constant_number_literal_u64() {
        let a = 100_000u64;
        let b = 100000;
        assert!(a == b, 1);
    }

    #[test]
    public fun load_constant_number_literal_u128() {
        let a = 100_000_000000000000u128;
        let b: u128 = 100000000000000000;
        assert!(a == b, 1);
    }
}
