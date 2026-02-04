// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Group operations of BLS12-381.
module sui::ristretto255;

use sui::bcs;
use sui::group_ops::{Self, Element};

/////////////////////////////////////////////
////// Elliptic curve operations //////

public struct Scalar has store {}
public struct Point has store {}

// Const elements.
const SCALAR_ZERO_BYTES: vector<u8> =
    x"0000000000000000000000000000000000000000000000000000000000000000";
const SCALAR_ONE_BYTES: vector<u8> =
    x"0000000000000000000000000000000000000000000000000000000000000001";
const IDENTITY_BYTES: vector<u8> =
    x"0000000000000000000000000000000000000000000000000000000000000000";
const GENERATOR_BYTES: vector<u8> =
    x"e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";

// Internal types used by group_ops' native functions.
const SCALAR_TYPE: u8 = 5;
const POINT_TYPE: u8 = 6;

///////////////////////////////
////// Scalar operations //////

public fun scalar_from_bytes(bytes: &vector<u8>): Element<Scalar> {
    group_ops::from_bytes(SCALAR_TYPE, *bytes, false)
}

public fun scalar_from_u64(x: u64): Element<Scalar> {
    let scalar: u256 = x as u256;
    let bytes = bcs::to_bytes(&scalar);
    group_ops::from_bytes(SCALAR_TYPE, bytes, true)
}

public fun scalar_zero(): Element<Scalar> {
    group_ops::from_bytes(SCALAR_TYPE, SCALAR_ZERO_BYTES, true)
}

public fun scalar_one(): Element<Scalar> {
    group_ops::from_bytes(SCALAR_TYPE, SCALAR_ONE_BYTES, true)
}

public fun scalar_add(e1: &Element<Scalar>, e2: &Element<Scalar>): Element<Scalar> {
    group_ops::add(SCALAR_TYPE, e1, e2)
}

public fun scalar_sub(e1: &Element<Scalar>, e2: &Element<Scalar>): Element<Scalar> {
    group_ops::sub(SCALAR_TYPE, e1, e2)
}

public fun scalar_mul(e1: &Element<Scalar>, e2: &Element<Scalar>): Element<Scalar> {
    group_ops::mul(SCALAR_TYPE, e1, e2)
}

/// Returns e2/e1, fails if a is zero.
public fun scalar_div(e1: &Element<Scalar>, e2: &Element<Scalar>): Element<Scalar> {
    group_ops::div(SCALAR_TYPE, e1, e2)
}

public fun scalar_neg(e: &Element<Scalar>): Element<Scalar> {
    scalar_sub(&scalar_zero(), e)
}

// Fails if e is zero.
public fun scalar_inv(e: &Element<Scalar>): Element<Scalar> {
    scalar_div(e, &scalar_one())
}

public fun hash_to_scalar(m: &vector<u8>): Element<Scalar> {
    group_ops::hash_to(SCALAR_TYPE, m)
}

/////////////////////////////////
////// Point operations //////

public fun point_from_bytes(bytes: &vector<u8>): Element<Point> {
    group_ops::from_bytes(POINT_TYPE, *bytes, false)
}

public fun identity(): Element<Point> {
    group_ops::from_bytes(POINT_TYPE, IDENTITY_BYTES, true)
}

public fun generator(): Element<Point> {
    group_ops::from_bytes(POINT_TYPE, GENERATOR_BYTES, true)
}

public fun point_add(e1: &Element<Point>, e2: &Element<Point>): Element<Point> {
    group_ops::add(POINT_TYPE, e1, e2)
}

public fun point_sub(e1: &Element<Point>, e2: &Element<Point>): Element<Point> {
    group_ops::sub(POINT_TYPE, e1, e2)
}

public fun point_mul(e1: &Element<Scalar>, e2: &Element<Point>): Element<Point> {
    group_ops::mul(POINT_TYPE, e1, e2)
}

/// Returns e2 / e1, fails if scalar is zero.
public fun point_div(e1: &Element<Scalar>, e2: &Element<Point>): Element<Point> {
    group_ops::div(POINT_TYPE, e1, e2)
}

public fun point_neg(e: &Element<Point>): Element<Point> {
    point_sub(&identity(), e)
}

public fun hash_to_point(m: &vector<u8>): Element<Point> {
    group_ops::hash_to(POINT_TYPE, m)
}

/// Let 'scalars' be the vector [s1, s2, ..., sn] and 'elements' be the vector [e1, e2, ..., en].
/// Returns s1*e1 + s2*e2 + ... + sn*en.
/// Aborts with `EInputTooLong` if the vectors are larger than 32 (may increase in the future).
public fun multi_scalar_multiplication(
    scalars: &vector<Element<Scalar>>,
    elements: &vector<Element<Point>>,
): Element<Point> {
    group_ops::multi_scalar_multiplication(POINT_TYPE, scalars, elements)
}

public fun verify_range_proof(proof: &vector<u8>, range: u8, commitments: &vector<Element<Point>>): bool {
    verify_bulletproof_ristretto255(proof, range, &commitments.map_ref!(|c| *c.bytes()))
}

native fun verify_bulletproof_ristretto255(
    proof: &vector<u8>,
    range: u8,
    commitments: &vector<vector<u8>>,
): bool;

#[test]
fun test_bulletproof() {
    let proof =
        x"e0048a98b0f1545cab04cfb43b995a7079a35ae4a472bb2fac4d679311ab7f104531a4107479a775f79155b3b814ad7b65c34deca847e8ef9339b97d61fb83ea4c5978e1e4dab81dc027a405699617fe938d718ebfbe165ac7cf6fb10029bd07d73e5c232000f7242fdec95e66f5b80eefa1a79d18d0d8f1c50c63b5529f41fb4311f94a701a7ce42a47772e21aa7cb01a269fa4db6195f93bbde35fd35f36270b095f3b3834ccef5d833ec94e8b66598436bf9a751970cfb3ecf7738a69bd1971049d97cdf9fcf1e9a29e5028008986b7d251196fb0f8c63316903a54b1ee4b6107a62a9af25bd4d5ba55866abb132d6056e36e3c8adb508245692cd2c5a4f797187a4de2a28d2e86cf3da9ec1fa6f224564c94e471829fbc4b60cf02953cfce36f48cbdec4e1310dd30994361b71ebc8ca2ad80eb2dab0e5da6be0088d30a584071ceb34b2eaa8b2e5a026e19114f02b7c48a826584208be3c5f50828c8753877d9496195a266a412f4104fa510b9f12e4ceb9e0786a201b4eeefcfad962b3f423ce632e46ca20033990fc8ac484cd8cde10dbdd639c16c3259060ceb46aa2f463c82cac924a37e1915725381fb2aa3cfe00652a71707eb20da99f7aa9fdb40e58ee93d11ccb71b30b8573a1498c3cf776a08945413ee19e7697b6c53191594c40649fa20880fed18145e64ab58726420737708e3136e17907d1b32f48258a5509608c49827c3bc14f2458148ad3d9b87352af617f4fd168be1547d4914069d04dedcfbce83908b249d30dcbf4da212861171ccaa94cf4262b8628f073de9d63080e0598cfb37f824f4111b68fe625b56362a951c62fd02839ca3437a002f2110c";
    let commitment = x"c026d2b1790b3391f991ad4a2ad62e3ae5db6da3eeb2280aa83bd6018fe3967b";
    assert!(verify_bulletproof_ristretto255(&proof, 32, &vector[commitment]));
}
