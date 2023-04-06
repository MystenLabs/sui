// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ed25519_tests {
    use sui::ed25519;

    #[test]
    fun test_ed25519_valid_sig() {
        // Test generated from https://github.com/MystenLabs/fastcrypto/blob/874bb52ccadf9800b3bc21e640449705d7ff9ab0/fastcrypto/src/tests/ed25519_tests.rs
        let msg = x"315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3";
        let pk = x"cc62332e34bb2d5cd69f60efbb2a36cb916c7eb458301ea36636c4dbb012bd88";
        let sig = x"cce72947906dbae4c166fc01fd096432784032be43db540909bc901dbc057992b4d655ca4f4355cf0868e1266baacf6919902969f063e74162f8f04bc4056105";

        let verify = ed25519::ed25519_verify(&sig, &pk, &msg);
        assert!(verify == true, 0);
    }

    #[test]
    fun test_ed25519_invalid_sig() {
        let msg = x"315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3";
        let pk = x"cc62332e34bb2d5cd69f60efbb2a36cb916c7eb458301ea36636c4dbb012bd88";
        let invalid_sig = x"cde72947906dbae4c166fc01fd096432784032be43db540909bc901dbc057992b4d655ca4f4355cf0868e1266baacf6919902969f063e74162f8f04bc4056105";

        let verify = ed25519::ed25519_verify(&invalid_sig, &pk, &msg);
        assert!(verify == false, 0);
    
        let pk = x"cc62332e34bb2d5cd69f60efbb2a36cb916c7eb458301ea36636c4dbb012bd88";
        let sig = x"cce72947906dbae4c166fc01fd096432784032be43db540909bc901dbc057992b4d655ca4f4355cf0868e1266baacf6919902969f063e74162f8f04bc4056105";
        let other_msg = x"415f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3";

        let verify = ed25519::ed25519_verify(&sig, &pk, &other_msg);
        assert!(verify == false, 0);
    }

    #[test]
    fun test_ed25519_invalid_pubkey() {
        let msg = x"315f5bdb76d078c43b8ac0064e4a0164612b1fce77c869345bfc94c75894edd3";
        let pk = x"";
        let sig = x"cce72947906dbae4c166fc01fd096432784032be43db540909bc901dbc057992b4d655ca4f4355cf0868e1266baacf6919902969f063e74162f8f04bc4056105";

        let verify = ed25519::ed25519_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }
}
