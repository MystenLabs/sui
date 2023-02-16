// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::address {
    use sui::hex;
    use std::ascii;
    use std::bcs;
    use std::string;

    /// The length of an address, in bytes
    const LENGTH: u64 = 20;

    // The largest integer that can be represented with 20 bytes
    const MAX: u256 = 1461501637330902918203684832716283019655932542975;

    /// Error from `from_bytes` when it is supplied too many or too few bytes.
    const EAddressParseError: u64 = 0;

    /// Error from `from_u256` when
    const EU256TooBigToConvertToAddress: u64 = 1;

    /// Convert `a` into a u256 by interpreting `a` as the bytes of a big-endian integer
    /// (e.g., `to_u256(0x1) == 1`)
    public native fun to_u256(a: address): u256;

    spec to_u256 {
        pragma opaque;
        // TODO: stub to be replaced by actual abort conditions if any
        aborts_if [abstract] true;
        // TODO: specify actual function behavior
    }

    /// Convert `n` into an address by encoding it as a big-endian integer (e.g., `from_u256(1) = @0x1`)
    /// Aborts if `n` > `MAX_ADDRESS`
    public native fun from_u256(n: u256): address;

    spec from_u256 {
        pragma opaque;
        // TODO: stub to be replaced by actual abort conditions if any
        aborts_if [abstract] true;
        // TODO: specify actual function behavior
    }

    /// Convert `bytes` into an address.
    /// Aborts with `EAddressParseError` if the length of `bytes` is not 20
    public native fun from_bytes(bytes: vector<u8>): address;

    spec from_bytes {
        pragma opaque;
        // TODO: stub to be replaced by actual abort conditions if any
        aborts_if [abstract] true;
        // TODO: specify actual function behavior
    }

    /// Convert `a` into BCS-encoded bytes.
    public fun to_bytes(a: address): vector<u8> {
        bcs::to_bytes(&a)
    }

    /// Convert `a` to a hex-encoded ASCII string
    public fun to_ascii_string(a: address): ascii::String {
        ascii::string(hex::encode(to_bytes(a)))
    }

    /// Convert `a` to a hex-encoded ASCII string
    public fun to_string(a: address): string::String {
        string::from_ascii(to_ascii_string(a))
    }

    /// Length of a Sui address in bytes
    public fun length(): u64 {
        LENGTH
    }

    /// Largest possible address
    public fun max(): u256 {
        MAX
    }

    spec module { pragma verify = false; }
}
