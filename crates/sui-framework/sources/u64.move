module sui::u64 {
    use std::vector;
    use sui::math;

    const ETOO_FEW_BYTES: u64 = 1;

    public fun from_bytes(bytes: vector<u8>): u64 {
        assert!(vector::length(&bytes) >= 8, ETOO_FEW_BYTES);

        let i: u8 = 0;
        let sum: u64 = 0;
        while (i < 8) {
            sum = sum + (*vector::borrow(&bytes, (i as u64)) as u64) * math::pow(2, (7 - i) * 8);
            i = i + 1;
        };

        sum
    }
}