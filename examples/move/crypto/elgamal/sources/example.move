// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of proving plaintext equivalence of two ElGamal ciphertexts.
module elgamal::example;

use sui::{bls12381::{Self, Scalar, G1}, group_ops::{bytes, equal, Element}, hash::blake2b256};

/// An encryption of group element m under pk is (r*G, r*pk + m) for random r.
public struct ElGamalEncryption has drop, store {
    ephemeral: Element<G1>,
    ciphertext: Element<G1>,
}

public fun elgamal_decrypt(sk: &Element<Scalar>, enc: &ElGamalEncryption): Element<G1> {
    let pk_r = bls12381::g1_mul(sk, &enc.ephemeral);
    bls12381::g1_sub(&enc.ciphertext, &pk_r)
}

/// Basic sigma protocol for proving equality of two ElGamal encryptions.
/// See https://crypto.stackexchange.com/questions/30010/is-there-a-way-to-prove-equality-of-plaintext-that-was-encrypted-using-different
public struct EqualityProof has drop, store {
    a1: Element<G1>,
    a2: Element<G1>,
    a3: Element<G1>,
    z1: Element<Scalar>,
    z2: Element<Scalar>,
}

fun fiat_shamir_challenge(
    pk1: &Element<G1>,
    pk2: &Element<G1>,
    enc1: &ElGamalEncryption,
    enc2: &ElGamalEncryption,
    a1: &Element<G1>,
    a2: &Element<G1>,
    a3: &Element<G1>,
): Element<Scalar> {
    let mut to_hash = vector::empty<u8>();
    to_hash.append(*bytes(pk1));
    to_hash.append(*bytes(pk2));
    to_hash.append(*bytes(&enc1.ephemeral));
    to_hash.append(*bytes(&enc1.ciphertext));
    to_hash.append(*bytes(&enc2.ephemeral));
    to_hash.append(*bytes(&enc2.ciphertext));
    to_hash.append(*bytes(a1));
    to_hash.append(*bytes(a2));
    to_hash.append(*bytes(a3));
    let mut hash = blake2b256(&to_hash);
    // Make sure we are in the right field. Note that for security we only need the lower 128 bits.
    *vector::borrow_mut(&mut hash, 0) = 0;
    bls12381::scalar_from_bytes(&hash)
}

public fun equility_verify(
    pk1: &Element<G1>,
    pk2: &Element<G1>,
    enc1: &ElGamalEncryption,
    enc2: &ElGamalEncryption,
    proof: &EqualityProof,
): bool {
    let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &proof.a1, &proof.a2, &proof.a3);
    // Check if z1*G = a1 + c*pk1
    let lhs = bls12381::g1_mul(&proof.z1, &bls12381::g1_generator());
    let pk1_c = bls12381::g1_mul(&c, pk1);
    let rhs = bls12381::g1_add(&proof.a1, &pk1_c);
    if (!equal(&lhs, &rhs)) {
        return false
    };
    // Check if z2*G = a2 + c*eph2
    let lhs = bls12381::g1_mul(&proof.z2, &bls12381::g1_generator());
    let eph2_c = bls12381::g1_mul(&c, &enc2.ephemeral);
    let rhs = bls12381::g1_add(&proof.a2, &eph2_c);
    if (!equal(&lhs, &rhs)) {
        return false
    };
    // Check if a3 = c*(ct2 - ct1) + z1*eph1 - z2*pk2
    let scalars = vector[c, bls12381::scalar_neg(&c), proof.z1, bls12381::scalar_neg(&proof.z2)];
    let points = vector[enc2.ciphertext, enc1.ciphertext, enc1.ephemeral, *pk2];
    let lhs = bls12381::g1_multi_scalar_multiplication(&scalars, &points);
    if (!equal(&lhs, &proof.a3)) {
        return false
    };
    return true
}

// The following is insecure since the nonce is small, but in practice it should be a random scalar.
#[test_only]
public fun insecure_elgamal_encrypt(pk: &Element<G1>, r: u64, m: &Element<G1>): ElGamalEncryption {
    let r = bls12381::scalar_from_u64(r);
    let ephemeral = bls12381::g1_mul(&r, &bls12381::g1_generator());
    let pk_r = bls12381::g1_mul(&r, pk);
    let ciphertext = bls12381::g1_add(m, &pk_r);
    ElGamalEncryption { ephemeral, ciphertext }
}

// The following is insecure since the nonces are small, but in practice they should be random scalars.
#[test_only]
public fun insecure_equility_prove(
    pk1: &Element<G1>,
    pk2: &Element<G1>,
    enc1: &ElGamalEncryption,
    enc2: &ElGamalEncryption,
    sk1: &Element<Scalar>,
    r1: u64,
    r2: u64,
): EqualityProof {
    let b1 = bls12381::scalar_from_u64(r1);
    let b2 = bls12381::scalar_from_u64(r1 + 1);
    let r2 = bls12381::scalar_from_u64(r2);

    // a1 = b1*G (for proving knowledge of sk1)
    let a1 = bls12381::g1_mul(&b1, &bls12381::g1_generator());
    // a2 = b2*g (for proving knowledge of r2)
    let a2 = bls12381::g1_mul(&b2, &bls12381::g1_generator());
    let scalars = vector[b1, bls12381::scalar_neg(&b2)];
    let points = vector[enc1.ephemeral, *pk2];
    let a3 = bls12381::g1_multi_scalar_multiplication(&scalars, &points);
    // RO challenge
    let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &a1, &a2, &a3);
    // z1 = b1 + c*sk1
    let z1 = bls12381::scalar_add(&bls12381::scalar_mul(&c, sk1), &b1);
    // z2 = b2 + c*r2
    let z2 = bls12381::scalar_add(&bls12381::scalar_mul(&c, &r2), &b2);

    EqualityProof { a1, a2, a3, z1, z2 }
}
