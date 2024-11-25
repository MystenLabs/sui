// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module groth16::example_tests;

use groth16::example::{create_verifying_key, create_proof, verify};
use sui::bls12381;

#[test]
fun test_verification() {
    let vk = create_verifying_key(
        bls12381::g1_from_bytes(
            &x"b58cfc3b0f43d98e7dbe865af692577d52813cb62ef3c355215ec3be2a0355a1ae5da76dd3e626f8a60de1f4a8138dee",
        ),
        bls12381::g2_from_bytes(
            &x"9047b42915b32ef9dffe3acc0121a1450416e7f9791159f165ab0729d744da3ed82cd4822ca1d7fef35147cfd620b59b0ca09db7dff43aab6c71635ba8f86a83f058e9922e5cdacbe21d0e5e855cf1e776a61b272c12272fe526f5ba3b48d579",
        ),
        bls12381::g2_from_bytes(
            &x"ad7c5a6cefcae53a3fbae72662c7c04a2f8e1892cb83615a02b32c31247172b7f317489b84e72f14acaf4f3e9ed18141157c6c1464bf15d957227f75a3c550d6d27f295b41a753340c6eec47b471b2cb8664c84f3e9b725325d3fb8afc6b56d0",
        ),
        vector[
            bls12381::g1_from_bytes(
                &x"b2c9c61ccc28e913284a47c34e60d487869ff423dd574db080d35844f9eddd2b2967141b588a35fa82a278ce39ae6b1a",
            ),
            bls12381::g1_from_bytes(
                &x"9026ae12d58d203b4fc5dfad4968cbf51e43632ed1a05afdcb2e380ee552b036fbefc7780afe9675bcb60201a2421b2c",
            ),
        ],
        bls12381::g2_from_bytes(
            &x"b1294927d02f8e86ac57c3b832f4ecf5e03230445a9a785ac8d25cf968f48cca8881d0c439c7e8870b66567cf611da0c1734316632f39d3125c8cecca76a8661db91cbfae217547ea1fc078a24a1a31555a46765011411094ec649d42914e2f5",
        ),
    );

    let public_inputs = vector[
        bls12381::scalar_from_bytes(
            &x"46722abc81a82d01ac89c138aa01a8223cb239ceb1f02cdaad7e1815eb997ca6",
        ),
    ];

    let proof = create_proof(
        bls12381::g1_from_bytes(
            &x"9913bdcabdff2cf1e7dea1673c5edba5ed6435df2f2a58d6d9e624609922bfa3976a98d991db333812bf6290a590afaa",
        ),
        bls12381::g2_from_bytes(
            &x"b0265b35af5069593ee88626cf3ba9a0f07699510a25aec3a27048792ab83b3467d6b814d1c09c412c4dcd7656582e6607b72915081c82794ccedf643c27abace5b23a442079d8dcbd0d68dd697b8e0b699a1925a5f2c77f5237efbbbeda3bd0",
        ),
        bls12381::g1_from_bytes(
            &x"b1237cf48ca7aa98507e826aac336b9e24f14133de1923fffac602a1203b795b3037c4c94e7246bacee7b2757ae912e5",
        ),
    );

    assert!(vk.prepare().verify(&proof, &public_inputs));
}
