// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A verifier for the Groth16 zk-SNARK over the BLS12-381 construction.
/// See https://eprint.iacr.org/2016/260.pdf for details.
module groth16::example;

use sui::{bls12381, group_ops::Element};

// === Types ===

/// A Groth16 proof.
public struct Proof has drop {
    a: Element<bls12381::G1>,
    b: Element<bls12381::G2>,
    c: Element<bls12381::G1>,
}

/// A Groth16 verifying key used to verify a zero-knowledge proof.
public struct VerifyingKey has store, drop {
    alpha: Element<bls12381::G1>,
    beta: Element<bls12381::G2>,
    gamma: Element<bls12381::G2>,
    gamma_abc: vector<Element<bls12381::G1>>,
    delta: Element<bls12381::G2>,
}

/// A prepared verifying key. This makes verification faster than using the verifying key directly.
public struct PreparedVerifyingKey has store, drop {
    alpha_beta: Element<bls12381::GT>,
    gamma_neg: Element<bls12381::G2>,
    gamma_abc: vector<Element<bls12381::G1>>,
    delta_neg: Element<bls12381::G2>,
}

// === Errors ===

#[error]
const EInvalidNumberOfPublicInputs: vector<u8> =
    b"There must be one more public input than gamma_abc entries in the verifying key.";

// === Public Functions ===

/// Create a new `Proof`.
public fun create_proof(
    a: Element<bls12381::G1>,
    b: Element<bls12381::G2>,
    c: Element<bls12381::G1>,
): Proof {
    Proof { a, b, c }
}

/// Create a new `VerifyingKey`.
public fun create_verifying_key(
    alpha: Element<bls12381::G1>,
    beta: Element<bls12381::G2>,
    gamma: Element<bls12381::G2>,
    gamma_abc: vector<Element<bls12381::G1>>,
    delta: Element<bls12381::G2>,
): VerifyingKey {
    VerifyingKey { alpha, beta, gamma, gamma_abc, delta }
}

/// Create a PreparedVerifyingKey from a VerifyingKey. This only have to be
/// done once.
public fun prepare(vk: VerifyingKey): PreparedVerifyingKey {
    PreparedVerifyingKey {
        alpha_beta: bls12381::pairing(&vk.alpha, &vk.beta),
        gamma_neg: bls12381::g2_neg(&vk.gamma),
        gamma_abc: vk.gamma_abc,
        delta_neg: bls12381::g2_neg(&vk.delta),
    }
}

/// Verify a Groth16 proof with some public inputs and a verifying key.
public fun verify(
    pvk: &PreparedVerifyingKey,
    proof: &Proof,
    public_inputs: &vector<Element<bls12381::Scalar>>,
): bool {
    let prepared_inputs = prepare_inputs(&pvk.gamma_abc, public_inputs);
    let mut lhs = bls12381::pairing(&proof.a, &proof.b);
    lhs = bls12381::gt_add(&lhs, &bls12381::pairing(&prepared_inputs, &pvk.gamma_neg));
    lhs = bls12381::gt_add(&lhs, &bls12381::pairing(&proof.c, &pvk.delta_neg));
    lhs == pvk.alpha_beta
}

// === Helpers ===

fun prepare_inputs(
    vk_gamma_abc: &vector<Element<bls12381::G1>>,
    public_inputs: &vector<Element<bls12381::Scalar>>,
): Element<bls12381::G1> {
    let length = public_inputs.length();
    assert!(length + 1 == vk_gamma_abc.length(), EInvalidNumberOfPublicInputs);

    let mut output = vk_gamma_abc[0];
    let mut i = 0;
    while (i < length) {
        output =
            bls12381::g1_add(
                &output,
                &bls12381::g1_mul(&public_inputs[i], &vk_gamma_abc[i + 1]),
            );
        i = i + 1;
    };
    output
}
