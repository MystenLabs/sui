// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::hex_tests {
    use sui::hex;

    #[test]
    fun test_hex_encode_string_literal() {
        assert!(b"30" == hex::encode(b"0"));
        assert!(b"61" == hex::encode(b"a"));
        assert!(b"666666" == hex::encode(b"fff"));
    }

    #[test]
    fun test_hex_encode_hex_literal() {
        assert!(b"ff" == hex::encode(x"ff"));
        assert!(b"fe" == hex::encode(x"fe"));
        assert!(b"00" == hex::encode(x"00"));
    }

    #[test]
    fun test_hex_decode_string_literal() {
        assert!(x"ff" == hex::decode(b"ff"));
        assert!(x"fe" == hex::decode(b"fe"));
        assert!(x"00" == hex::decode(b"00"));
    }

    #[test]
    fun test_hex_decode_string_literal__lowercase_and_uppercase() {
        assert!(x"ff" == hex::decode(b"Ff"));
        assert!(x"ff" == hex::decode(b"fF"));
        assert!(x"ff" == hex::decode(b"FF"));
    }

    #[test]
    fun test_hex_decode_string_literal__long_hex() {
        assert!(
            x"036d2416252ae1db8aedad59e14b007bee6ab94a3e77a3549a81137871604456f3" == hex::decode(
                b"036d2416252ae1Db8aedAd59e14b007bee6aB94a3e77a3549a81137871604456f3"
            ),
        );
    }

    #[test]
    #[expected_failure(abort_code = hex::EInvalidHexLength)]
    fun test_hex_decode__invalid_length() {
        hex::decode(b"0");
    }

    #[test]
    #[expected_failure(abort_code = hex::ENotValidHexCharacter)]
    fun test_hex_decode__hex_literal() {
        hex::decode(x"ffff");
    }

    #[test]
    #[expected_failure(abort_code = hex::ENotValidHexCharacter)]
    fun test_hex_decode__invalid_string_literal() {
        hex::decode(b"0g");
    }
}
