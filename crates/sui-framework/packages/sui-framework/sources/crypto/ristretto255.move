// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Group operations of BLS12-381.
/// Only available in devnet.
module sui::ristretto255;

use sui::bcs;
use sui::group_ops::{Self, Element};

/////////////////////////////////////////////
////// Elliptic curve operations //////

public struct Scalar has store {}
public struct G has store {}

// Scalars are encoded using little-endian byte order and is always 32 bytes.
// Points are encoded as described in https://www.rfc-editor.org/rfc/rfc9496.html#name-encode.

// Const elements.
const SCALAR_ZERO_BYTES: vector<u8> =
    x"0000000000000000000000000000000000000000000000000000000000000000";
const SCALAR_ONE_BYTES: vector<u8> =
    x"0100000000000000000000000000000000000000000000000000000000000000";
const IDENTITY_BYTES: vector<u8> =
    x"0000000000000000000000000000000000000000000000000000000000000000";
const GENERATOR_BYTES: vector<u8> =
    x"e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";

// Internal types used by group_ops' native functions.
const SCALAR_TYPE: u8 = 5;
const G_TYPE: u8 = 6;

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

/////////////////////////////////
////// Point operations //////

public fun g_from_bytes(bytes: &vector<u8>): Element<G> {
    group_ops::from_bytes(G_TYPE, *bytes, false)
}

public fun g_identity(): Element<G> {
    group_ops::from_bytes(G_TYPE, IDENTITY_BYTES, true)
}

public fun g_generator(): Element<G> {
    group_ops::from_bytes(G_TYPE, GENERATOR_BYTES, true)
}

public fun g_add(e1: &Element<G>, e2: &Element<G>): Element<G> {
    group_ops::add(G_TYPE, e1, e2)
}

public fun g_sub(e1: &Element<G>, e2: &Element<G>): Element<G> {
    group_ops::sub(G_TYPE, e1, e2)
}

public fun g_mul(e1: &Element<Scalar>, e2: &Element<G>): Element<G> {
    group_ops::mul(G_TYPE, e1, e2)
}

/// Returns e2 / e1, fails if scalar is zero.
public fun g_div(e1: &Element<Scalar>, e2: &Element<G>): Element<G> {
    group_ops::div(G_TYPE, e1, e2)
}

public fun g_neg(e: &Element<G>): Element<G> {
    g_sub(&g_identity(), e)
}
