// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module for Ristretto255 group operations.
module sui::ristretto255 {

    use sui::group_ops;
    use sui::group_ops::Element;

    struct Scalar {}
    struct G {}

    // Encoding follows https://datatracker.ietf.org/doc/html/draft-hdevalence-cfrg-ristretto-01. Elements are
    // encoded using little-endian byte order and points are compressed.

    // Const elements.
    const SCALAR_ZERO_BYTES: vector<u8> = x"0000000000000000000000000000000000000000000000000000000000000000";
    const SCALAR_ONE_BYTES: vector<u8> = x"0100000000000000000000000000000000000000000000000000000000000000";
    const G_IDENTITY_BYTES: vector<u8> = x"0000000000000000000000000000000000000000000000000000000000000000";
    const G_GENERATOR_BYTES: vector<u8> = x"e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";

    // Internal types used by group_ops' native functions.
    const SCALAR_TYPE: u8 = 0;
    const G_TYPE: u8 = 1;

    ///////////////////////////////
    ////// Scalar operations //////

    public fun scalar_from_bytes(bytes: &vector<u8>): Element<Scalar> {
        group_ops::from_bytes(SCALAR_TYPE, bytes, false)
    }

    public fun scalar_from_u64(x: u64): Element<Scalar> {
        let bytes = SCALAR_ZERO_BYTES;
        group_ops::set_as_prefix(x, false, &mut bytes);
        group_ops::from_bytes(SCALAR_TYPE, &bytes, true)
    }

    public fun scalar_zero(): Element<Scalar> {
        group_ops::from_bytes(SCALAR_TYPE, &SCALAR_ZERO_BYTES, true)
    }

    public fun scalar_one(): Element<Scalar> {
        group_ops::from_bytes(SCALAR_TYPE, &SCALAR_ONE_BYTES, true)
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

    // Returns e2 / e1, fails if a is zero.
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

    // TODO: msm? is it useful?

    ////////////////////////////////
    ////// G group operations //////

    public fun g_from_bytes(bytes: &vector<u8>): Element<G> {
        group_ops::from_bytes(G_TYPE, bytes, false)
    }

    public fun g_identity(): Element<G> {
        group_ops::from_bytes(G_TYPE, &G_IDENTITY_BYTES, true)
    }

    public fun g_generator(): Element<G> {
        group_ops::from_bytes(G_TYPE, &G_GENERATOR_BYTES, true)
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

    // Returns e / scalar, fails if scalar is zero.
    public fun g_div(e1: &Element<Scalar>, e2: &Element<G>): Element<G> {
        group_ops::div(G_TYPE, e1, e2)
    }

    public fun g_neg(e: &Element<G>): Element<G> {
        g_sub(&g_identity(), e)
    }

    // Computes SHA512(m) to get 64 bytes that are passed to from_uniform_bytes (see RFC).
    public fun hash_to_g(m: &vector<u8>): Element<G> {
        group_ops::hash_to(G_TYPE, m)
    }

    // Let 'scalars' be the vector [s1, s2, ..., sn] and 'elements' be the vector [e1, e2, ..., en].
    // Returns s1*e1 + s2*e2 + ... + sn*en.
    public fun g_multi_scalar_multiplication(scalars: &vector<Element<Scalar>>, elements: &vector<Element<G>>): Element<G> {
        group_ops::multi_scalar_multiplication(G_TYPE, scalars, elements)
    }
}
