// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecdsa_r1_tests {
    use sui::ecdsa_r1;

    #[test]
    fun test_ecrecover_pubkey() {
        // test case generated against https://github.com/MystenLabs/fastcrypto/blob/285e3f238112703cdfb7eb21e0ea3100e2882e14/fastcrypto/src/tests/secp256r1_recoverable_tests.rs
        let msg = b"Hello, world!";

        // recover with Keccak256
        let sig = x"209841acabd0fdf6d25fa0948b2f5c6c74f930eede89c6fce59502218a3134a147535921bf0cdf5d1990f6f0b3dadb8c05069ddc531db057d325857ce198a52900";
        let pubkey_bytes = x"0227322b3a891a0a280d6bc1fb2cbb23d28f54906fd6407f5f741f6def5762609a";
        let pubkey = ecdsa_r1::secp256r1_ecrecover(&sig, &msg, 0);
        assert!(pubkey == pubkey_bytes);

        // recover with Sha256
        let sig = x"26d84720652d8bc4ddd1986434a10b3b7b69f0e35a17c6a5987e6d1cba69652f4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb200";
        let pubkey = ecdsa_r1::secp256r1_ecrecover(&sig, &msg, 1);
        assert!(pubkey == pubkey_bytes);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa_r1::EInvalidSignature)]
    fun test_ecrecover_pubkey_invalid_sig() {
        let msg = b"Hello, world!";
        let sig = x"26d84720652d8bc4ddd1986434a10b3b7b69f0e35a17c6a5987e6d1cba69652f4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb2";
        ecdsa_r1::secp256r1_ecrecover(&sig, &msg, 1);
    }

    #[test]
    fun test_secp256r1_verify_fails_with_recoverable_sig() {
        let msg = b"Hello, world!";
        let pk = x"0227322b3a891a0a280d6bc1fb2cbb23d28f54906fd6407f5f741f6def5762609a";
        // signature is a 65-byte recoverable one with recovery id 0
        let sig = x"209841acabd0fdf6d25fa0948b2f5c6c74f930eede89c6fce59502218a3134a147535921bf0cdf5d1990f6f0b3dadb8c05069ddc531db057d325857ce198a52900";
        let verify = ecdsa_r1::secp256r1_verify(&sig, &pk, &msg, 0);
        assert!(verify == false);

        // signature is a 65-byte recoverable one with recovery id 1
        let sig = x"209841acabd0fdf6d25fa0948b2f5c6c74f930eede89c6fce59502218a3134a147535921bf0cdf5d1990f6f0b3dadb8c05069ddc531db057d325857ce198a52901";
        let verify = ecdsa_r1::secp256r1_verify(&sig, &pk, &msg, 0);
        assert!(verify == false);
    }

    #[test]
    fun test_secp256r1_verify_success_with_nonrecoverable_sig() {
        let msg = b"Hello, world!";
        let pk = x"0227322b3a891a0a280d6bc1fb2cbb23d28f54906fd6407f5f741f6def5762609a";

        let sig = x"209841acabd0fdf6d25fa0948b2f5c6c74f930eede89c6fce59502218a3134a147535921bf0cdf5d1990f6f0b3dadb8c05069ddc531db057d325857ce198a529";
        let verify = ecdsa_r1::secp256r1_verify(&sig, &pk, &msg, 0);
        assert!(verify == true);

        let sig = x"26d84720652d8bc4ddd1986434a10b3b7b69f0e35a17c6a5987e6d1cba69652f4384a342487642df5e44592d304bea0ceb0fae2e347fa3cec5ce1a8144cfbbb2";
        let verify = ecdsa_r1::secp256r1_verify(&sig, &pk, &msg, 1);
        assert!(verify == true);
    }

    #[test]
    fun test_secp256r1_invalid_public_key_length() {
        let msg = b"Hello, world!";
        let pk = x"0227322b3a891a0a280d6bc1fb2cbb23d28f54906fd6407f5f741f6def576260";

        let sig = x"209841acabd0fdf6d25fa0948b2f5c6c74f930eede89c6fce59502218a3134a147535921bf0cdf5d1990f6f0b3dadb8c05069ddc531db057d325857ce198a529";
        let verify = ecdsa_r1::secp256r1_verify(&sig, &pk, &msg, 0);
        assert!(verify == false);
    }
}
