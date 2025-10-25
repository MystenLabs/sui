// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::poseidon_tests;

use std::u64::min;
use sui::poseidon::poseidon_bn254;

#[test]
fun test_poseidon_bn254_hash() {
    let msg = vector[1u256];
    let expected =
        18586133768512220936620570745912940619677854269274689475585506675881198879027u256;
    let actual = poseidon_bn254(&msg);
    assert!(actual == expected);

    let msg = vector[1u256, 2u256];
    let expected = 7853200120776062878684798364095072458815029376092732009249414926327459813530u256;
    let actual = poseidon_bn254(&msg);
    assert!(actual == expected);

    let msg = vector[0u256];
    let expected =
        19014214495641488759237505126948346942972912379615652741039992445865937985820u256;
    let actual = poseidon_bn254(&msg);
    assert!(actual == expected);

    let msg = vector[
        1u256,
        2u256,
        3u256,
        4u256,
        5u256,
        6u256,
        7u256,
        8u256,
        9u256,
        10u256,
        11u256,
        12u256,
        13u256,
        14u256,
        15u256,
    ];
    let expected = 4203130618016961831408770638653325366880478848856764494148034853759773445968u256;
    let actual = poseidon_bn254(&msg);
    assert!(actual == expected);
}

#[test]
fun test_poseidon_bn254_canonical_input() {
    // Scalar field size minus 1.
    let msg = vector[
        21888242871839275222246405745257275088548364400416034343698204186575808495616u256,
    ];
    let expected = 3366645945435192953002076803303112651887535928162668198103357554665518664470u256;
    let actual = poseidon_bn254(&msg);
    assert!(actual == expected);
}

#[test]
#[expected_failure(abort_code = sui::poseidon::ENonCanonicalInput)]
fun test_poseidon_bn254_non_canonical_input() {
    // Scalar field size.
    let msg = vector[
        21888242871839275222246405745257275088548364400416034343698204186575808495617u256,
    ];
    poseidon_bn254(&msg);
}

#[test]
#[expected_failure(abort_code = sui::poseidon::EEmptyInput)]
fun test_poseidon_bn254_empty_input() {
    let msg = vector[];
    poseidon_bn254(&msg);
}

#[test]
#[expected_failure(abort_code = sui::poseidon::ETooManyInputs)]
fun test_poseidon_bn254_too_many_inputs() {
    let msg = vector[
        1u256,
        2u256,
        3u256,
        4u256,
        5u256,
        6u256,
        7u256,
        8u256,
        9u256,
        10u256,
        11u256,
        12u256,
        13u256,
        14u256,
        15u256,
        16u256,
        17u256,
    ];
    poseidon_bn254(&msg);
}

#[test]
fun test_poseidon_bn254_tree() {
    let msg = vector[
        0u256,
        1u256,
        2u256,
        3u256,
        4u256,
        5u256,
        6u256,
        7u256,
        8u256,
        9u256,
        10u256,
        11u256,
        12u256,
        13u256,
        14u256,
        15u256,
        16u256,
        17u256,
        18u256,
        19u256,
        20u256,
        21u256,
        22u256,
        23u256,
        24u256,
        25u256,
        26u256,
        27u256,
        28u256,
        29u256,
    ];

    // Construct tree with a fixed arity
    let mut msg = msg;
    let mut next = vector[];
    let k = 16; // arity

    while (msg.length() > 1) {
        msg.reverse();
        while (!msg.is_empty()) {
            let mut chunk = vector[];
            min(k, msg.length()).do!(|_| {
                chunk.push_back(msg.pop_back());
            });
            next.push_back(poseidon_bn254(&chunk));
        };
        msg = next;
        next = vector[];
    };

    let expected = 4123755143677678663754455867798672266093104048057302051129414708339780424023u256;
    assert!(msg[0] == expected);
}
