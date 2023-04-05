module 0x42::M {
    fun t() {
        // Single underscore separations allowed
        let _ = 8_5u128;
        let _ = 8_5;
        let _: u8 = 8_5;
        let _ = 0x8_5u128;
        let _ = 0x8_5;
        let _: u8 = 0x8_5;

        // Multiple underscore separations allowed
        let _ = 02345677_15463636363_36464784848_456847568568775u256;
        let _ = 0_1_3_4;
        let _: u64 = 0_1_3_4;
        let _ = 0x02345677_15463636363_36464784848_456847568568775u256;
        let _ = 0x0_1_3_4;
        let _: u64 = 0x0_1_3_4;

        // Single trailing allowed
        let _ = 567_u64;
        let _ = 567_;
        let _: u64 = 5_6_7;
        let _ = 0x567_u64;
        let _ = 0x567_;
        let _: u64 = 0x5_6_7;

        // Multiple trailing allowed
        let _ = 567___u32;
        let _ = 567___;
        let _: u64 = 567___;
        let _ = 0x567___u32;
        let _ = 0x567___;
        let _: u64 = 0x567___;

        // Multiple underscore in tandem allowed
        let _ = 0__8u16;
        let _ = 0__8;
        let _: u8 = 0__8;
        let _ = 0x0__8u16;
        let _ = 0x0__8;
        let _: u8 = 0x0__8;
    }
}
