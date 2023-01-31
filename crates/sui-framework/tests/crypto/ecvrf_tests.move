// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ecvrf_tests {
    use sui::ecvrf;

    #[test]
    fun test_ecvrf_verify() {
        let alpha_string = b"Hello, world!";

        // Test vector produced with Fastcrypto
        let public_key = x"782be3a5b858bf96faa5368695fd856d2467aab704fc20d153e7e76def9de550";
        let output = x"5fcb766b85074089e3061c2b3fbe8b601b634afd8c5e3e10f40b55f2bf9c0f0b27b49a146d50ede2ffa880ba5c9d36b8b3e47597cc02f2f9794b8910eedb767d";
        let proof = x"f6f31ef3ccd04877ff3e8c8fd9de6f8216ac8318b5b48db57bce8e3b0072023cce91578bf03161c0ac5637149cba89e597eef2d361d1c767e339f62a5f889b7ebfde27bad20041035ae9fea2d522660f";

        assert!(ecvrf::ecvrf_verify(&output, &alpha_string, &public_key, &proof), 0);
    }

}