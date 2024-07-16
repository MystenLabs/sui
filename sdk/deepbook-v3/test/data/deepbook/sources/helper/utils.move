// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Deepbook utility functions.
module deepbook::utils {
    /// Pop elements from the back of `v` until its length equals `n`,
    /// returning the elements that were popped in the order they
    /// appeared in `v`.
    public(package) fun pop_until<T>(v: &mut vector<T>, n: u64): vector<T> {
        let mut res = vector[];
        while (v.length() > n) {
            res.push_back(v.pop_back());
        };

        res.reverse();
        res
    }

    /// Pop `n` elements from the back of `v`, returning the elements
    /// that were popped in the order they appeared in `v`.
    ///
    /// Aborts if `v` has fewer than `n` elements.
    public(package) fun pop_n<T>(v: &mut vector<T>, mut n: u64): vector<T> {
        let mut res = vector[];
        while (n > 0) {
            res.push_back(v.pop_back());
            n = n - 1;
        };

        res.reverse();
        res
    }

    /// first bit is 0 for bid, 1 for ask
    /// next 63 bits are price (assertion for price is done in order function)
    /// last 64 bits are order_id
    public(package) fun encode_order_id(is_bid: bool, price: u64, order_id: u64): u128 {
        if (is_bid) {
            ((price as u128) << 64) + (order_id as u128)
        } else {
            (1u128 << 127) + ((price as u128) << 64) + (order_id as u128)
        }
    }

    /// Decode order_id into (is_bid, price, order_id)
    public(package) fun decode_order_id(encoded_order_id: u128): (bool, u64, u64) {
        let is_bid = (encoded_order_id >> 127) == 0;
        let price = (encoded_order_id >> 64) as u64;
        let price = price & ((1u64 << 63) - 1);
        let order_id = (encoded_order_id & ((1u128 << 64) - 1)) as u64;

        (is_bid, price, order_id)
    }

    #[test]
    fun test_encode_decode_order_id() {
        let is_bid = true;
        let price = 2371538230592318123;
        let order_id = 9211238512301581235;
        let encoded_order_id = encode_order_id(is_bid, price, order_id);
        let (decoded_is_bid, decoded_price, decoded_order_id) = decode_order_id(encoded_order_id);
        assert!(decoded_is_bid == is_bid, 0);
        assert!(decoded_price == price, 0);
        assert!(decoded_order_id == order_id, 0);

        let is_bid = false;
        let price = 1;
        let order_id = 1;
        let encoded_order_id = encode_order_id(is_bid, price, order_id);
        let (decoded_is_bid, decoded_price, decoded_order_id) = decode_order_id(encoded_order_id);
        assert!(decoded_is_bid == is_bid, 0);
        assert!(decoded_price == price, 0);
        assert!(decoded_order_id == order_id, 0);

        let is_bid = true;
        let price = (1u128 << 64 - 1) as u64 - 1;
        let order_id = (1u128 << 64 - 1) as u64;
        let encoded_order_id = encode_order_id(is_bid, price, order_id);
        let (decoded_is_bid, decoded_price, decoded_order_id) = decode_order_id(encoded_order_id);
        assert!(decoded_is_bid == is_bid, 0);
        assert!(decoded_price == price, 0);
        assert!(decoded_order_id == order_id, 0);

        let is_bid = false;
        let price = 0;
        let order_id = 0;
        let encoded_order_id = encode_order_id(is_bid, price, order_id);
        let (decoded_is_bid, decoded_price, decoded_order_id) = decode_order_id(encoded_order_id);
        assert!(decoded_is_bid == is_bid, 0);
        assert!(decoded_price == price, 0);
        assert!(decoded_order_id == order_id, 0);
    }
}
