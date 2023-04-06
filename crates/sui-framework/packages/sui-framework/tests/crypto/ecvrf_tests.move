// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecvrf_tests {
    use sui::ecvrf;

    #[test]
    fun test_ecvrf_verify() {
        // Test vector produced with Fastcrypto
        let output = x"4fad431c7402fa1d4a7652e975aeb9a2b746540eca0b1b1e59c8d19c14a7701918a8249136e355455b8bc73851f7fc62c84f2e39f685b281e681043970026ed8";
        let alpha_string = b"Hello, world!";
        let public_key = x"1ea6f0f467574295a2cd5d21a3fd3a712ade354d520d3bd0fe6088d7b7c2e00e";
        let proof = x"d8ad2eafb4f2eaf317447726e541359f26dfce248431fe09984fdc73144abb6ceb006c57a29a742eae5a81dd04239870769e310a81046cbbaff8b0bd27a6d6affee167ebba50549b58ffdf9aa192f506";
        assert!(ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof), 0);
    }

    #[test]
    fun test_ecvrf_invalid() {
        let output = b"invalid hash, invalid hash, invalid hash, invalid hash, invalid ";
        let alpha_string = b"Hello, world!";
        let public_key = x"1ea6f0f467574295a2cd5d21a3fd3a712ade354d520d3bd0fe6088d7b7c2e00e";
        let proof = x"d8ad2eafb4f2eaf317447726e541359f26dfce248431fe09984fdc73144abb6ceb006c57a29a742eae5a81dd04239870769e310a81046cbbaff8b0bd27a6d6affee167ebba50549b58ffdf9aa192f506";
        assert!(!ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof), 1);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidHashLength)]
    fun test_invalid_length() {
        let output = b"invalid hash";
        let alpha_string = b"Hello, world!";
        let public_key = x"1ea6f0f467574295a2cd5d21a3fd3a712ade354d520d3bd0fe6088d7b7c2e00e";
        let proof = x"d8ad2eafb4f2eaf317447726e541359f26dfce248431fe09984fdc73144abb6ceb006c57a29a742eae5a81dd04239870769e310a81046cbbaff8b0bd27a6d6affee167ebba50549b58ffdf9aa192f506";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidPublicKeyEncoding)]
    fun test_invalid_public_key() {
        let output = x"4fad431c7402fa1d4a7652e975aeb9a2b746540eca0b1b1e59c8d19c14a7701918a8249136e355455b8bc73851f7fc62c84f2e39f685b281e681043970026ed8";
        let alpha_string = b"Hello, world!";
        let public_key = b"invalid public key";
        let proof = x"d8ad2eafb4f2eaf317447726e541359f26dfce248431fe09984fdc73144abb6ceb006c57a29a742eae5a81dd04239870769e310a81046cbbaff8b0bd27a6d6affee167ebba50549b58ffdf9aa192f506";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

    #[test]
    #[expected_failure(abort_code = ecvrf::EInvalidProofEncoding)]
    fun test_invalid_proof() {
        let output = x"4fad431c7402fa1d4a7652e975aeb9a2b746540eca0b1b1e59c8d19c14a7701918a8249136e355455b8bc73851f7fc62c84f2e39f685b281e681043970026ed8";
        let alpha_string = b"Hello, world!";
        let public_key = x"1ea6f0f467574295a2cd5d21a3fd3a712ade354d520d3bd0fe6088d7b7c2e00e";
        let proof = b"invalid proof";
        ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof);
    }

}
