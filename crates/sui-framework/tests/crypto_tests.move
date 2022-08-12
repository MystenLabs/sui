// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::crypto_tests {
    use sui::crypto;
    #[test]
    fun test_ecrecover_pubkey() {
        // test case generated against https://docs.rs/secp256k1/latest/secp256k1/
        let hashed_msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146, 162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];
        let sig = vector[132, 220, 128, 67, 151, 154, 45, 143, 50, 56, 176, 134, 137, 58, 223, 166, 191, 230, 178, 184, 123, 11, 19, 69, 59, 205, 72, 206, 153, 187, 184, 7, 16, 74, 73, 45, 38, 238, 81, 96, 138, 225, 235, 143, 95, 142, 185, 56, 99, 3, 97, 27, 66, 99, 79, 225, 139, 21, 67, 254, 78, 251, 176, 176, 0];
        let pubkey_bytes = vector[2, 2, 87, 224, 47, 124, 255, 117, 223, 91, 188, 190, 151, 23, 241, 173, 148, 107, 20, 103, 63, 155, 108, 151, 251, 152, 205, 205, 239, 71, 224, 86, 9];

        let pubkey = crypto::ecrecover(sig, hashed_msg);
        assert!(pubkey == pubkey_bytes, 0);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_ecrecover_pubkey_fail_to_recover() {
        let hashed_msg = vector[0];
        let sig = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        crypto::ecrecover(sig, hashed_msg);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_ecrecover_pubkey_invalid_sig() {
        let hashed_msg = vector[0];
        // incorrect length sig
        let sig = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        crypto::ecrecover(sig, hashed_msg);
    }

    #[test]
    fun test_keccak256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146, 162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let hashed_msg = crypto::keccak256(msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);
    }
}
