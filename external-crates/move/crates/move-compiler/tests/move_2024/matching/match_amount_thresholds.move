module 0x2a::M {
    const MIN_AMOUNT: u64 = 100;
    const MAX_AMOUNT: u64 = 1_000_000;

    public fun classify(amount: u64): u8 {
        match (amount) {
            MIN_AMOUNT => 0,
            MAX_AMOUNT => 1,
            n @ _ if (*n > MAX_AMOUNT) => 2,
            _ => 3,
        }
    }
}
