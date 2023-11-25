// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module scratch_off::math {

    /// Hard coding this as 1 billion
    const MODULO_FACTOR: u256 = 1_000_000_000;

    public fun bytes_to_u256(random_32b: &vector<u8>): u256 {
        let output = 0;
        let bytes_length = 32;
        let idx = 0;
        while (idx < bytes_length) {
            let current_byte = *std::vector::borrow(random_32b, idx);
            output = (output << 8) | (current_byte as u256) ;
            idx = idx + 1;
        };
        output
    }

    public fun get_random_u64(random_32b: &vector<u8>): u64 {
        let output = bytes_to_u256(random_32b);
        (output as u64)
    }

    public fun get_random_u64_in_range(random_32b: &vector<u8>, max_range: u64): u64 {
        let output = bytes_to_u256(random_32b);
        (output as u64) % max_range
    }

    /// Example inputs here are 
    /// random_number_result = 12340
    /// prizes_odds = 2500 (25%)
    /// total_lot = 10000
    /// 12340 % 10000 = 1234 < 2500 which is true.
    public fun should_draw_prize(
        random_32b: &vector<u8>,
        winning_ticket_amount: u64, 
        total_tickets: u64
    ): bool {
        let random_number_result = get_random_u64(random_32b);
        // This assumes that mod can hit 0.
        (random_number_result % total_tickets) < winning_ticket_amount
    }
}