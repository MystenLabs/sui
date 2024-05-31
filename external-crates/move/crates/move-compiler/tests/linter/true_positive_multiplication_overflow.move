module 0x42::TruePositiveTests {
    public fun u64_overflow() {
        let a: u64 = 1_000_000_000_000_000;
        let b: u64 = 3_000_000_000_000_000;
        let _ = a * b; // Should trigger a warning
    }

    public fun u128_overflow() {
        let _a: u128 = 340282366920938463463374607431768211455; // (2^128 - 1) / 2
        let _b: u128 = 3;
        let _ = _a * _b; // Should trigger a warning
    }

    public fun mixed_type_overflow() {
        let _a: u64 = 18446744073709551615; // u64::MAX
        let _b: u8 = 2;
        let _ = (_a as u128) * (_b as u128); // Should trigger a warning
    }
}
