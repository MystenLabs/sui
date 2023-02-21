// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::address_spec {
    use sui::address;

    const MAX: u256 = 1461501637330902918203684832716283019655932542975;

    public fun from_bytes(bytes: vector<u8>): address {
        address::from_bytes(bytes)
    }

    spec from_bytes {
        aborts_if len(bytes) != 20;
        ensures result == address::from_bytes(bytes);

        let addr = @0x89b9f9d1fadc027cf9532d6f99041522; //$t1
        let expected_output = x"0000000089b9f9d1fadc027cf9532d6f99041522"; //$t2

        aborts_if len(expected_output) != 20;
        aborts_if address::from_bytes(expected_output) != addr;
    }

    public fun to_u256(a: address): u256 {
        address::to_u256(a)
    }

    spec to_u256 {
        aborts_if false;
        ensures address::from_u256(result) == a;
    }

    public fun from_u256(n: u256): address {
        address::from_u256(n)
    }

    spec from_u256 {
        aborts_if n > MAX;
        ensures address::to_u256(result) == n;
    }
}
