module sui::nizk;

use sui::group_ops::Element;
use sui::ristretto255::{Self, Point, Scalar};

public struct NIZK has drop {
    a: Element<Point>,
    b: Element<Point>,
    z: Element<Scalar>,
}

#[test_only]
public fun new(a: Element<Point>, b: Element<Point>, z: Element<Scalar>): NIZK {
    NIZK { a, b, z }
}

public fun from_bytes(bytes: vector<u8>): NIZK {
    let mut bcs = sui::bcs::new(bytes);
    let a = ristretto255::point_from_bytes(&peel_tuple_u8(&mut bcs, 32));
    let b = ristretto255::point_from_bytes(&peel_tuple_u8(&mut bcs, 32));
    let z = ristretto255::scalar_from_bytes(&peel_tuple_u8(&mut bcs, 32));
    NIZK { a, b, z }
}

// TODO: If fixed length vectors are ever supported, we should use that instead.
fun peel_tuple_u8(bcs: &mut sui::bcs::BCS, length: u64): vector<u8> {
    vector::tabulate!(length, |_| bcs.peel_u8())
}

fun challenge(
    dst: &vector<u8>,
    h: &Element<Point>,
    x_g: &Element<Point>,
    x_h: &Element<Point>,
    a: &Element<Point>,
    b: &Element<Point>,
): Element<Scalar> {
    // TODO: Align with fastcrypto
    let mut bytes: vector<u8> = vector::empty();
    bytes.append(x"00000000"); // length of dst - todo: make variable
    bytes.append(*dst);

    // TODO: In fastcrypto, these are added as a tuple and bcs encoded. Ensure that this is the same here:
    bytes.append(*ristretto255::generator().bytes());
    bytes.append(*h.bytes());
    bytes.append(*x_g.bytes());
    bytes.append(*x_h.bytes());
    bytes.append(*a.bytes());
    bytes.append(*b.bytes());

    ristretto255::hash_to_scalar(&sui::hash::sha3_512(&bytes))
}

public fun verify(
    proof: &NIZK,
    dst: &vector<u8>, // sui-
    h: &Element<Point>,
    x_g: &Element<Point>,
    x_h: &Element<Point>,
): bool {
    assert!(
        h != ristretto255::identity() && x_g != ristretto255::identity() && x_h != ristretto255::identity(),
    );

    let challenge = challenge(dst, h, x_g, x_h, &proof.a, &proof.b);

    is_valid_relation(
                &proof.a,
                x_g,
                &ristretto255::generator(),
                &proof.z,
                &challenge,
            ) && is_valid_relation(
                &proof.b,
                x_h,
                h,
                &proof.z,
                &challenge,
                )
}

/// Checks if e1 + c e2 = z e3
fun is_valid_relation(
    e1: &Element<Point>,
    e2: &Element<Point>,
    e3: &Element<Point>,
    z: &Element<Scalar>,
    c: &Element<Scalar>,
): bool {
    ristretto255::point_add(e1, &ristretto255::point_mul(c, e2)) == ristretto255::point_mul(z, e3)
}

#[test_only]
public fun prove(
    dst: &vector<u8>,
    x: &Element<Scalar>,
    h: &Element<Point>,
    x_g: &Element<Point>,
    x_h: &Element<Point>,
    r: &Element<Scalar>,
): NIZK {
    let a = ristretto255::point_mul(r, &ristretto255::generator());
    let b = ristretto255::point_mul(r, h);
    let c = challenge(dst, h, x_g, x_h, &a, &b);
    let z = ristretto255::scalar_add(r, &ristretto255::scalar_mul(&c, x));
    NIZK { a, b, z }
}

#[test]
fun prove_nizk_round_trip() {
    let tuple1 = ristretto255::point_mul(
        &ristretto255::scalar_from_u64(3),
        &ristretto255::generator(),
    );
    let tuple2 = ristretto255::point_mul(
        &ristretto255::scalar_from_u64(4),
        &ristretto255::generator(),
    );
    let tuple3 = ristretto255::point_mul(
        &ristretto255::scalar_from_u64(12),
        &ristretto255::generator(),
    );
    let dst = b"sui-nizk-test";

    let nizk = prove(
        &dst,
        &ristretto255::scalar_from_u64(4),
        &tuple1,
        &tuple2,
        &tuple3,
        &ristretto255::scalar_from_u64(91011), // randomness
    );

    assert!(nizk.verify(&dst, &tuple1, &tuple2, &tuple3));
}
