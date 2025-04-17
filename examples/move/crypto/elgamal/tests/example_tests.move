// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module elgamal::tests;

use elgamal::example;
use sui::{bls12381::{Self, Scalar, G1}, group_ops::Element, random, test_utils::assert_eq};

#[test]
fun test_elgamal_equality() {
    let mut gen = random::new_generator_for_testing();

    // We have two parties.
    let (sk1, pk1) = insecure_elgamal_key_gen(gen.generate_u64());
    let (_, pk2) = insecure_elgamal_key_gen(gen.generate_u64());

    // A sender wishes to send an encrypted message to pk1.
    let m = bls12381::g1_mul(
        &bls12381::scalar_from_u64(gen.generate_u64()),
        &bls12381::g1_generator(),
    );
    let enc1 = example::insecure_elgamal_encrypt(&pk1, gen.generate_u64(), &m);

    // The first party decrypts the message.
    let m1 = example::elgamal_decrypt(&sk1, &enc1);
    assert_eq(m, m1);

    // Now, the first party wishes to send an encrypted message to pk2.
    let r2 = gen.generate_u64();
    let enc2 = example::insecure_elgamal_encrypt(&pk2, r2, &m);
    // And to prove equality of the two encrypted messages.
    let proof = example::insecure_equility_prove(
        &pk1,
        &pk2,
        &enc1,
        &enc2,
        &sk1,
        gen.generate_u64(),
        r2,
    );

    // Anyone can verify it.
    assert!(example::equility_verify(&pk1, &pk2, &enc1, &enc2, &proof), 0);

    // Proving with an invalid witness should result in a failed verification.
    let bad_r2 = r2 + 1;
    let proof = example::insecure_equility_prove(
        &pk1,
        &pk2,
        &enc1,
        &enc2,
        &sk1,
        gen.generate_u64(),
        bad_r2,
    );
    assert!(!example::equility_verify(&pk1, &pk2, &enc1, &enc2, &proof), 0);
}

// The following is insecure since the secret key is small, but in practice it should be a random scalar.
fun insecure_elgamal_key_gen(sk: u64): (Element<Scalar>, Element<G1>) {
    let sk = bls12381::scalar_from_u64(sk);
    let pk = bls12381::g1_mul(&sk, &bls12381::g1_generator());
    (sk, pk)
}
