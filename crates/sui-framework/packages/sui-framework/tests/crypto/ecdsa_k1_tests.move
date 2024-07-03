// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecdsa_k1_tests {
    use sui::ecdsa_k1;
    use sui::hash;

    #[test]
    fun test_ecrecover_pubkey() {
        // test case generated against https://github.com/MystenLabs/fastcrypto/blob/f9e64dc028040f863a53a6a88072bda71abd9946/fastcrypto/src/tests/secp256k1_recoverable_tests.rs
        let msg = b"Hello, world!";

        // recover with keccak256 hash
        let sig = x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd88600";
        let pubkey_bytes = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let pubkey = ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 0);
        assert!(pubkey == pubkey_bytes);

        // recover with sha256 hash
        let sig = x"e5847245b38548547f613aaea3421ad47f5b95a222366fb9f9b8c57568feb19c7077fc31e7d83e00acc1347d08c3e1ad50a4eeb6ab044f25c861ddc7be5b8f9f01";
        let pubkey_bytes = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let pubkey = ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 1);
        assert!(pubkey == pubkey_bytes);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EFailToRecoverPubKey)]
    fun test_ecrecover_pubkey_fail_to_recover() {
        let msg = x"00";
        let sig = x"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 1);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EInvalidSignature)]
    fun test_ecrecover_pubkey_invalid_sig() {
        let msg = b"Hello, world!";
        // incorrect length sig
        let sig = x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd886";
        ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 1);
    }

    #[test]
    fun test_secp256k1_verify_fails_with_recoverable_sig() {
        let msg = b"Hello, world!";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let sig = x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd88600";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 0);
        assert!(verify == false);

        let sig_1 = x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd88601";
        let verify_1 = ecdsa_k1::secp256k1_verify(&sig_1, &pk, &msg, 0);
        assert!(verify_1 == false);
    }

    #[test]
    fun test_secp256k1_verify_success_with_nonrecoverable_sig() {
        let msg = b"Hello, world!";
        // verify with keccak256 hash
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let sig = x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd886";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 0);
        assert!(verify == true);

        // verify with sha256 hash
        let sig = x"e5847245b38548547f613aaea3421ad47f5b95a222366fb9f9b8c57568feb19c7077fc31e7d83e00acc1347d08c3e1ad50a4eeb6ab044f25c861ddc7be5b8f9f";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 1);
        assert!(verify == true);
    }

    #[test]
    fun test_secp256k1_invalid() {
        let msg = b"Hello, world!";
        let sig = x"e5847245b38548547f613aaea3421ad47f5b95a222366fb9f9b8c57568feb19c7077fc31e7d83e00acc1347d08c3e1ad50a4eeb6ab044f25c861ddc7be5b8f9f";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 1);
        assert!(verify == false);

        let sig = x"e5847245b38548547f613aaea3421ad47f5b95a222366fb9f9b8c57568feb19c7077fc31e7d83e00acc1347d08c3e1ad50a4eeb6ab044f25c861ddc7be5b8f9f";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 2);
        assert!(verify == false);
    }

    #[test]
    fun test_ecrecover_eth_address() {
        // Test case from https://stackoverflow.com/questions/67278243/how-to-verify-the-signature-made-by-metamask-for-ethereum
        let sig = x"382a3e04daf88f322730f6a2972475fc5646ea8c4a7f3b5e83a90b10ba08a7364cd2f55348f2b6d210fbed7fc485abf19ecb2f3967e410d6349dd7dd1d4487751b";
        let msg = x"19457468657265756d205369676e6564204d6573736167653a0a3533307836336639613932643864363162343861396666663864353830383034323561333031326430356338696777796b3472316f376f";
        let addr1 = x"63f9a92d8d61b48a9fff8d58080425a3012d05c8";
        let addr = ecrecover_eth_address(sig, msg);
        assert!(addr == addr1);
    }

    // Helper Move function to recover signature directly to an ETH address.
    fun ecrecover_eth_address(mut sig: vector<u8>, msg: vector<u8>): vector<u8> {
        // Normalize the last byte of the signature to be 0 or 1.
        let v = &mut sig[64];
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };

        let pubkey = ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 0);

        let uncompressed = ecdsa_k1::decompress_pubkey(&pubkey);

        // Take the last 64 bytes of the uncompressed pubkey.
        let mut uncompressed_64 = vector<u8>[];
        let mut i = 1;
        while (i < 65) {
            let value = &uncompressed[i];
            uncompressed_64.push_back(*value);
            i = i + 1;
        };

        // Take the last 20 bytes of the hash of the 64-bytes uncompressed pubkey.
        let hashed = hash::keccak256(&uncompressed_64);
        let mut addr = vector<u8>[];
        let mut i = 12;
        while (i < 32) {
            let value = &hashed[i];
            addr.push_back(*value);
            i = i + 1;
        };

        addr
    }

    #[test]
    fun test_sign() {
        let msg = b"Hello, world!";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let sk = x"42258dcda14cf111c602b8971b8cc843e91e46ca905151c02744a6b017e69316";

        // Test with Keccak256 hash
        let sig = ecdsa_k1::secp256k1_sign(&sk, &msg, 0, false);
        assert!(ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 0));
        assert!(sig == x"7e4237ebfbc36613e166bfc5f6229360a9c1949242da97ca04867e4de57b2df30c8340bcb320328cf46d71bda51fcb519e3ce53b348eec62de852e350edbd886");

        // Test with SHA256 hash
        let sig = ecdsa_k1::secp256k1_sign(&sk, &msg, 1, false);
        assert!(ecdsa_k1::secp256k1_verify(&sig, &pk, &msg, 1));
        assert!(sig == x"e5847245b38548547f613aaea3421ad47f5b95a222366fb9f9b8c57568feb19c7077fc31e7d83e00acc1347d08c3e1ad50a4eeb6ab044f25c861ddc7be5b8f9f");

        // Verification should fail with another message
        let other_msg = b"Farewell, world!";
        assert!(!ecdsa_k1::secp256k1_verify(&sig, &pk, &other_msg, 0));
    }

    #[test]
    fun test_sign_recoverable() {
        let msg = b"Hello, world!";
        let pk = x"02337cca2171fdbfcfd657fa59881f46269f1e590b5ffab6023686c7ad2ecc2c1c";
        let sk = x"42258dcda14cf111c602b8971b8cc843e91e46ca905151c02744a6b017e69316";

        // Test with Keccak256 hash
        let sig = ecdsa_k1::secp256k1_sign(&sk, &msg, 0, true);
        assert!(pk == ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 0));

        // Test with SHA256 hash
        let sig = ecdsa_k1::secp256k1_sign(&sk, &msg, 1, true);
        assert!(pk == ecdsa_k1::secp256k1_ecrecover(&sig, &msg, 1));

        // Recoveres pk should not be the same with another message
        let other_msg = b"Farewell, world!";
        assert!(pk != ecdsa_k1::secp256k1_ecrecover(&sig, &other_msg, 0));
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EInvalidHashFunction)]
    fun test_sign_invalid_hash() {
        let msg = b"Hello, world!";
        let sk = x"42258dcda14cf111c602b8971b8cc843e91e46ca905151c02744a6b017e69316";

        // Invalid hash function
        ecdsa_k1::secp256k1_sign(&sk, &msg, 2, false);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EInvalidPrivKey)]
    fun test_sign_invalid_private_key() {
        let msg = b"Hello, world!";

        // Invalid (too short) private key
        let sk = x"42258dcda14cf111c602b8971b8cc843e91e46ca905151c02744a6b017e693";

        ecdsa_k1::secp256k1_sign(&sk, &msg, 0, false);
    }

    #[test]
    fun test_generate_keypair() {
        let seed = b"Some random seed, 32 bytes long.";
        let kp = ecdsa_k1::secp256k1_keypair_from_seed(&seed);

        let msg = b"Hello, world!";

        let sig = ecdsa_k1::secp256k1_sign(kp.private_key(), &msg, 0, false);
        assert!(ecdsa_k1::secp256k1_verify(&sig, kp.public_key(), &msg, 0));

        let sig = ecdsa_k1::secp256k1_sign(kp.private_key(), &msg, 1, false);
        assert!(ecdsa_k1::secp256k1_verify(&sig, kp.public_key(), &msg, 1));
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EInvalidSeed)]
    fun test_generate_keypair_invalid_seed() {
        let seed = b"Seed is not 32 bytes long";
        ecdsa_k1::secp256k1_keypair_from_seed(&seed);
    }

}
