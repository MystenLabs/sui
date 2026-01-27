// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(implicit_const_copy), test_only]
module sui::ristretto255_tests;

use sui::ristretto255;

#[test]
fun test_ristretto255_arithmetic() {
    let x = ristretto255::scalar_from_u64(3);
    let y = ristretto255::point_mul(&x, &ristretto255::generator());
    let z = ristretto255::point_add(&ristretto255::generator(), &ristretto255::point_add(&ristretto255::generator(), &ristretto255::generator()));
    assert!(y == z)
}

#[test]
fun test_ristretto255_arithmetic_2() {
    let z = ristretto255::point_add(&ristretto255::generator(), &ristretto255::generator());
}


#[test]
fun test_ristretto255_arithmetic_3() {
    let x = ristretto255::scalar_from_u64(3);
    let y = ristretto255::scalar_from_u64(4);
    let z = ristretto255::scalar_from_u64(7);
    assert!(ristretto255::scalar_add(&x, &y) == z);
}
