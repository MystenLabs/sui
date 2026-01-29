module sui::nizk;

use sui::group_ops::Element;
use sui::ristretto255::{Self, Point, Scalar};

public struct NIZK {
    a: Element<Point>,
    b: Element<Point>,
    z: Element<Scalar>,
}

public fun verify(
    proof: &NIZK,
    dst: &vector<u8>,
    h: &Element<Point>,
    x_g: &Element<Point>,
    x_h: &Element<Point>,
): bool {
    assert!(
        h != ristretto255::identity() && x_g != ristretto255::identity() && x_h != ristretto255::identity(),
    );

    let mut bytes: vector<u8> = vector::empty();
    bytes.append(*dst);
    bytes.append(*ristretto255::generator().bytes());
    bytes.append(*h.bytes());
    bytes.append(*x_g.bytes());
    bytes.append(*x_h.bytes());

    let challenge = ristretto255::hash_to_scalar(&bytes);

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
