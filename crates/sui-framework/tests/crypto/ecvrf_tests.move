// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecvrf_tests {
    use sui::ecvrf;

    #[test]
    fun test_ecvrf_verify() {
        // Test vector produced with Fastcrypto
        let output = x"4ad05aafdaf4f0c76ac1ec46507431de2d81876ff95dd097334d2c257492afe41fe21900ef04e4e1f377143e24e38cf104fc5f983d28f940d4c0721d0823a3af";
        let alpha_string = b"Hello, world!";
        let public_key = x"2cf96313347d5f1f347464d1f59889b5c736a17e19bf899827aa3400733ea44d";
        let proof = x"445b02b99b1484e22325b0ad609f666df3d7099c90ed82a15d731ec42603fa4279ef4781ec2b88355dc08f5cbdf6cdae511836d5b5a5566a12fc56db4d8c3f9bf1c85186719706282c525df4b0477502";
        assert!(ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof), 0);
    }

    #[test]
    fun test_ecvrf_invalid() {
        let output = b"invalid hash, invalid hash, invalid hash, invalid hash, invalid ";
        let alpha_string = b"Hello, world!";
        let public_key = x"2cf96313347d5f1f347464d1f59889b5c736a17e19bf899827aa3400733ea44d";
        let proof = x"445b02b99b1484e22325b0ad609f666df3d7099c90ed82a15d731ec42603fa4279ef4781ec2b88355dc08f5cbdf6cdae511836d5b5a5566a12fc56db4d8c3f9bf1c85186719706282c525df4b0477502";
        assert!(!ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof), 1);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidHashLength)]
    fun test_invalid_length() {
        let output = b"invalid hash";
        let alpha_string = b"Hello, world!";
        let public_key = x"2cf96313347d5f1f347464d1f59889b5c736a17e19bf899827aa3400733ea44d";
        let proof = x"445b02b99b1484e22325b0ad609f666df3d7099c90ed82a15d731ec42603fa4279ef4781ec2b88355dc08f5cbdf6cdae511836d5b5a5566a12fc56db4d8c3f9bf1c85186719706282c525df4b0477502";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidPublicKeyEncoding)]
    fun test_invalid_public_key() {
        let output = x"4ad05aafdaf4f0c76ac1ec46507431de2d81876ff95dd097334d2c257492afe41fe21900ef04e4e1f377143e24e38cf104fc5f983d28f940d4c0721d0823a3af";
        let alpha_string = b"Hello, world!";
        let public_key = b"invalid public key";
        let proof = x"445b02b99b1484e22325b0ad609f666df3d7099c90ed82a15d731ec42603fa4279ef4781ec2b88355dc08f5cbdf6cdae511836d5b5a5566a12fc56db4d8c3f9bf1c85186719706282c525df4b0477502";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidProofEncoding)]
    fun test_invalid_proof() {
        let output = x"4ad05aafdaf4f0c76ac1ec46507431de2d81876ff95dd097334d2c257492afe41fe21900ef04e4e1f377143e24e38cf104fc5f983d28f940d4c0721d0823a3af";
        let alpha_string = b"Hello, world!";
        let public_key = x"2cf96313347d5f1f347464d1f59889b5c736a17e19bf899827aa3400733ea44d";
        let proof = b"invalid proof";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

}
