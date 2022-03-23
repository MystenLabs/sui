// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::UTF8 {
    use Std::ASCII;
    use Std::Option::Option;

    /// Wrapper type that should be interpreted as a UTF8 string by clients
    struct String has store, copy, drop {
        bytes: vector<u8>
    }

    // TODO: also include validating constructor
    /// Construct a UTF8 string from `bytes`. Does not
    /// perform any validation
    public fun string_unsafe(bytes: vector<u8>): String {
        String { bytes }
    }

    /// Construct a UTF8 string from the ASCII string `s`
    public fun from_ascii(s: ASCII::String): String {
        String { bytes: ASCII::into_bytes(s) }
    }

    /// Try to convert `self` to an ASCCI string
    public fun try_into_ascii(self: String): Option<ASCII::String> {
        ASCII::try_string(self.bytes)
    }

    /// Return the underlying bytes of `self`
    public fun bytes(self: &String): &vector<u8> {
        &self.bytes
    }

    /// Consume `self` and return its underlying bytes
    public fun into_bytes(self: String): vector<u8> {
        let String { bytes } = self;
        bytes
    }
}
