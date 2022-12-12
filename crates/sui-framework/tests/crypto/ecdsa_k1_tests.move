// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecdsa_tests {
    use sui::ecdsa_k1;
    use std::vector;
    
    #[test]
    fun test_ecrecover_pubkey() {
        // test case generated against https://docs.rs/secp256k1/latest/secp256k1/
        let hashed_msg = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";

        let sig = x"84dc8043979a2d8f3238b086893adfa6bfe6b2b87b0b13453bcd48ce99bbb807104a492d26ee51608ae1eb8f5f8eb9386303611b42634fe18b1543fe4efbb0b000";
        let pubkey_bytes = x"020257e02f7cff75df5bbcbe9717f1ad946b14673f9b6c97fb98cdcdef47e05609";

        let pubkey = ecdsa_k1::ecrecover(&sig, &hashed_msg);
        assert!(pubkey == pubkey_bytes, 0);
    }

    #[test]
    fun test_ecrecover_pubkey_2() {
        // Test case from go-ethereum: https://github.com/ethereum/go-ethereum/blob/master/crypto/signature_test.go#L37
        let hashed_msg = x"ce0677bb30baa8cf067c88db9811f4333d131bf8bcf12fe7065d211dce971008";
        let sig = x"90f27b8b488db00b00606796d2987f6a5f59ae62ea05effe84fef5b8b0e549984a691139ad57a3f0b906637673aa2f63d1f55cb1a69199d4009eea23ceaddc9301";
        let pubkey_bytes = x"02e32df42865e97135acfb65f3bae71bdc86f4d49150ad6a440b6f15878109880a";

        let pubkey = ecdsa_k1::ecrecover(&sig, &hashed_msg);
        assert!(pubkey == pubkey_bytes, 0);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EFailToRecoverPubKey)]
    fun test_ecrecover_pubkey_fail_to_recover() {
        let hashed_msg = x"00";
        let sig = x"0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        ecdsa_k1::ecrecover(&sig, &hashed_msg);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_k1::EInvalidSignature)]
    fun test_ecrecover_pubkey_invalid_sig() {
        let hashed_msg = x"ce0677bb30baa8cf067c88db9811f4333d131bf8bcf12fe7065d211dce971008";
        // incorrect length sig
        let sig = x"90f27b8b488db00b00606796d2987f6a5f59ae62ea05effe84fef5b8b0e549984a691139ad57a3f0b906637673aa2f63d1f55cb1a69199d4009eea23ceaddc93";
        ecdsa_k1::ecrecover(&sig, &hashed_msg);
    }

    #[test]
    fun test_secp256k1_valid_sig() {
        let msg = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let pk = x"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let sig = x"9c7a72ff1e7db1646b9f9443cb1a3563aa3a6344e4e513efb96258c7676ac4895953629d409a832472b710a028285dfec4733a2c1bb0a2749e465a18292b8bd601";

        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == true, 0)
    }

    #[test]
    fun test_secp256k1_invalid_sig() {
        let msg = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let pk = x"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        // sig in the form of (r, s, 0) instead of (r, s, 1)
        let sig = x"9c7a72ff1e7db1646b9f9443cb1a3563aa3a6344e4e513efb96258c7676ac4895953629d409a832472b710a028285dfec4733a2c1bb0a2749e465a18292b8bd600";

        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_sig_length() {
        let msg = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let pk = x"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let sig = x"9c7a72ff1e7db1646b9f9443cb1a3563aa3a6344e4e513efb96258c7676ac4895953629d409a832472b710a028285dfec4733a2c1bb0a2749e465a18292b8bd6";
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_hashed_msg_length() {
        let msg = x"01";
        let pk = x"0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let sig = x"9c7a72ff1e7db1646b9f9443cb1a3563aa3a6344e4e513efb96258c7676ac4895953629d409a832472b710a028285dfec4733a2c1bb0a2749e465a18292b8bd6";
        
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_public_key_length() {
        let msg = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let pk = x"79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let sig = x"9c7a72ff1e7db1646b9f9443cb1a3563aa3a6344e4e513efb96258c7676ac4895953629d409a832472b710a028285dfec4733a2c1bb0a2749e465a18292b8bd601";
        
        let verify = ecdsa_k1::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_ecrecover_eth_address() {
        // Test case from https://web3js.readthedocs.io/en/v1.7.5/web3-eth-accounts.html#recover
        let sig = x"b91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c";
        let hashed_msg = x"1da44b586eb0729ff70a73c326926f6ed5a25f5b056e7f47fbc6e58d86871655";

        let addr1 = x"2c7536e3605d9c16a7a3d7b1898e529396a65c23";
        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);

        // Test case from https://etherscan.io/verifySig/9754
        let sig = x"cb614cba67d6a37b9cb90d21635d81ed035b8ccb99f0befe05495b819111119b17ecf0c0cb4bcc781de387206f6dfcd9f1b99e1b54b44c376412d8f5c919b1981b";
        let hashed_msg = x"1da44b586eb0729ff70a73c326926f6ed5a25f5b056e7f47fbc6e58d86871655";
        let addr1 = x"4cbf668fca6f10d01f161122534044436b80702e";
        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);

        // Test case from https://goerli.etherscan.io/tx/0x18f72457b356f367db214de9dda07f5d253ebfeb5c426b0d9d5b346b4ba8d021
        let sig = x"8e809da5ca76e6371ba8dcaa748fc2973f0d9862f76ed08f55b869f5e73591dd24a7367f1ee9e6e3723d13bb0a7092fafb8851f7eecd4a8d34c977013e1551482e";
        let hashed_msg = x"529283629f75203330f0acf68bdbc4e879047fe75da8071c079c495bbb9fb78a";
        let addr1 = x"4cbf668fca6f10d01f161122534044436b80702e";
        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);
    }

    #[test]
    fun test_keccak256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = x"57caa176af1ac0433c5df30e8dabcd2ec1af1e92a26eced5f719b88458777cd6";
        let hashed_msg = ecdsa_k1::keccak256(&msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);
    }

    // Helper Move function to recover signature directly to an ETH address.
    fun ecrecover_eth_address(sig: vector<u8>, hashed_msg: vector<u8>): vector<u8> {
        // Normalize the last byte of the signature to be 0 or 1.
        let v = vector::borrow_mut(&mut sig, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };

        let pubkey = ecdsa_k1::ecrecover(&sig, &hashed_msg);
        let uncompressed = ecdsa_k1::decompress_pubkey(&pubkey);

        // Take the last 64 bytes of the uncompressed pubkey.
        let uncompressed_64 = vector::empty<u8>();
        let i = 1;
        while (i < 65) {
            let value = vector::borrow(&uncompressed, i);
            vector::push_back(&mut uncompressed_64, *value);
            i = i + 1;
        };

        // Take the last 20 bytes of the hash of the 64-bytes uncompressed pubkey.
        let hashed = ecdsa_k1::keccak256(&uncompressed_64);
        let addr = vector::empty<u8>();
        let i = 12;
        while (i < 32) {
            let value = vector::borrow(&hashed, i);
            vector::push_back(&mut addr, *value);
            i = i + 1;
        };
        addr
    }
}
