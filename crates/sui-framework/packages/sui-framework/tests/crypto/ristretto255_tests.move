// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(implicit_const_copy), test_only]
module sui::ristretto255_tests;

use std::unit_test::assert_eq;
use sui::group_ops;
use sui::random;
use sui::ristretto255;

const ORDER_BYTES: vector<u8> = x"edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
const ORDER_MINUS_ONE_BYTES: vector<u8> =
    x"ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
const LONG_SCALAR_BYTES: vector<u8> =
    x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff0000000000";
const SHORT_SCALAR_BYTES: vector<u8> =
    x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff0000";
const LONG_G_BYTES: vector<u8> =
    x"44f53520926ec81fbd5a387845beb7df85a96a24ece18738bdcfa6a7822a176dbb";
const SHORT_G_BYTES: vector<u8> = x"44f53520926ec81fbd5a387845beb7df85a96a24ece18738bdcfa6a7822a17";

#[test]
fun test_scalar_ops() {
    let zero = ristretto255::scalar_from_u64(0);
    let one = ristretto255::scalar_from_u64(1);
    assert!(group_ops::equal(&zero, &ristretto255::scalar_zero()));
    assert!(group_ops::equal(&one, &ristretto255::scalar_one()));
    assert!(!group_ops::equal(&zero, &ristretto255::scalar_one()));
    let zero0 = ristretto255::scalar_mul(&zero, &one);
    assert!(group_ops::equal(&zero0, &ristretto255::scalar_zero()));

    let two = ristretto255::scalar_add(&one, &one);
    let four = ristretto255::scalar_add(&two, &two);
    assert!(group_ops::equal(&four, &ristretto255::scalar_from_u64(4)));

    let eight = ristretto255::scalar_mul(&four, &two);
    assert!(group_ops::equal(&eight, &ristretto255::scalar_from_u64(8)));

    let zero0 = ristretto255::scalar_mul(&zero, &eight);
    assert!(group_ops::equal(&zero0, &ristretto255::scalar_zero()));

    let eight2 = ristretto255::scalar_mul(&eight, &one);
    assert!(group_ops::equal(&eight2, &ristretto255::scalar_from_u64(8)));

    let six = ristretto255::scalar_sub(&eight, &two);
    assert!(group_ops::equal(&six, &ristretto255::scalar_from_u64(6)));

    let minus_six = ristretto255::scalar_sub(&two, &eight);
    let three = ristretto255::scalar_add(&minus_six, &ristretto255::scalar_from_u64(9));
    assert!(group_ops::equal(&three, &ristretto255::scalar_from_u64(3)));

    let three = ristretto255::scalar_div(&two, &six);
    assert!(group_ops::equal(&three, &ristretto255::scalar_from_u64(3)));

    let minus_three = ristretto255::scalar_neg(&three);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_add(&minus_three, &six),
            &ristretto255::scalar_from_u64(3),
        ),
    );

    let minus_zero = ristretto255::scalar_neg(&zero);
    assert!(group_ops::equal(&minus_zero, &zero));

    let inv_three = ristretto255::scalar_inv(&three);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_mul(&six, &inv_three),
            &ristretto255::scalar_from_u64(2),
        ),
    );

    let order_minus_one = ristretto255::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
    let _ = ristretto255::scalar_add(&order_minus_one, &order_minus_one);
    let _ = ristretto255::scalar_mul(&order_minus_one, &order_minus_one);
}

#[test]
fun test_scalar_more_ops() {
    let mut gen = random::new_generator_for_testing();
    let x = gen.generate_u32() as u64;
    let x_scalar = ristretto255::scalar_from_u64(x);
    let y = gen.generate_u32() as u64;
    let y_scalar = ristretto255::scalar_from_u64(y);

    // Since x, y are u32 numbers, the following operations do not overflow as u64.
    assert!(
        group_ops::equal(
            &ristretto255::scalar_from_u64(x + y),
            &ristretto255::scalar_add(&x_scalar, &y_scalar),
        ),
    );
    let z_scalar = ristretto255::scalar_sub(&x_scalar, &y_scalar);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_from_u64(x),
            &ristretto255::scalar_add(&z_scalar, &y_scalar),
        ),
    );
    assert!(
        group_ops::equal(
            &ristretto255::scalar_from_u64(x * y),
            &ristretto255::scalar_mul(&x_scalar, &y_scalar),
        ),
    );
    let z_scalar = ristretto255::scalar_div(&x_scalar, &y_scalar);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_from_u64(y),
            &ristretto255::scalar_mul(&z_scalar, &x_scalar),
        ),
    );
    let z_scalar = ristretto255::scalar_neg(&x_scalar);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_zero(),
            &ristretto255::scalar_add(&x_scalar, &z_scalar),
        ),
    );
    let z_scalar = ristretto255::scalar_inv(&x_scalar);
    assert!(
        group_ops::equal(
            &ristretto255::scalar_one(),
            &ristretto255::scalar_mul(&x_scalar, &z_scalar),
        ),
    );

    let mut i = 0u64;
    let mut z = ristretto255::scalar_add(&x_scalar, &y_scalar);
    while (i < 20) {
        let mut new_z = ristretto255::scalar_mul(&z, &x_scalar);
        new_z = ristretto255::scalar_add(&new_z, &y_scalar);
        // check back
        let mut rev = ristretto255::scalar_sub(&new_z, &y_scalar);
        rev = ristretto255::scalar_div(&x_scalar, &rev);
        assert!(group_ops::equal(&z, &rev));

        let rev_as_bytes = *group_ops::bytes(&rev);
        let rev_scalar2 = ristretto255::scalar_from_bytes(&rev_as_bytes);
        assert!(group_ops::equal(&rev_scalar2, &rev));
        z = new_z;
        i = i + 1;
    };
}

#[test]
fun test_scalar_to_bytes_regression() {
    let zero = ristretto255::scalar_from_u64(0);
    let zero_bytes = *group_ops::bytes(&zero);
    let expected = x"0000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(expected, zero_bytes);

    let eight = ristretto255::scalar_from_u64(8);
    let eight_bytes = *group_ops::bytes(&eight);
    let expected = x"0800000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(expected, eight_bytes);

    let minus_one = ristretto255::scalar_sub(&zero, &ristretto255::scalar_from_u64(1));
    let minus_one_bytes = *group_ops::bytes(&minus_one);
    let expected = x"ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    assert_eq!(expected, minus_one_bytes);

    let minus_eight = ristretto255::scalar_sub(&zero, &ristretto255::scalar_from_u64(8));
    let minus_eight_bytes = *group_ops::bytes(&minus_eight);
    let expected = x"e5d3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    assert_eq!(expected, minus_eight_bytes);
}

#[test]
fun test_valid_scalar_from_bytes() {
    let eight = ristretto255::scalar_from_u64(8);
    let eight_from_bytes = ristretto255::scalar_from_bytes(group_ops::bytes(&eight));
    assert!(group_ops::equal(&eight, &eight_from_bytes));

    let zero = ristretto255::scalar_zero();
    let zero_from_bytes = ristretto255::scalar_from_bytes(group_ops::bytes(&zero));
    assert!(group_ops::equal(&zero, &zero_from_bytes));
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_order() {
    let _ = ristretto255::scalar_from_bytes(&ORDER_BYTES);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_empty() {
    let _ = ristretto255::scalar_from_bytes(&vector[]);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_too_short() {
    let _ = ristretto255::scalar_from_bytes(&SHORT_SCALAR_BYTES);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_too_long() {
    let _ = ristretto255::scalar_from_bytes(&LONG_SCALAR_BYTES);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_div() {
    let a = ristretto255::scalar_from_u64(0);
    let b = ristretto255::scalar_from_u64(10);
    let _ = ristretto255::scalar_div(&a, &b);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_scalar_inv() {
    let a = ristretto255::scalar_from_u64(0);
    let _ = ristretto255::scalar_inv(&a);
}

#[test]
fun test_g_ops() {
    let id = ristretto255::g_identity();
    let g = ristretto255::g_generator();

    assert!(group_ops::equal(&id, &ristretto255::g_sub(&g, &g)));
    assert!(group_ops::equal(&id, &ristretto255::g_sub(&id, &id)));
    assert!(group_ops::equal(&g, &ristretto255::g_add(&id, &g)));
    assert!(group_ops::equal(&g, &ristretto255::g_add(&g, &id)));

    let two_g = ristretto255::g_add(&g, &g);
    let four_g = ristretto255::g_add(&two_g, &two_g);

    let another_four_g = ristretto255::g_add(&id, &four_g);
    assert!(group_ops::equal(&four_g, &another_four_g));

    let another_four_g = ristretto255::g_mul(&ristretto255::scalar_from_u64(4), &g);
    assert!(group_ops::equal(&four_g, &another_four_g));

    let another_id = ristretto255::g_mul(&ristretto255::scalar_from_u64(0), &g);
    assert!(group_ops::equal(&id, &another_id));

    let another_two_g = ristretto255::g_sub(&four_g, &two_g);
    assert!(group_ops::equal(&two_g, &another_two_g));

    let another_two_g = ristretto255::g_div(&ristretto255::scalar_from_u64(2), &four_g);
    assert!(group_ops::equal(&two_g, &another_two_g));

    let minus_two_g = ristretto255::g_neg(&two_g);
    let another_two_g = ristretto255::g_add(&minus_two_g, &four_g);
    assert!(group_ops::equal(&two_g, &another_two_g));

    let order_minus_one = ristretto255::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
    let _ = ristretto255::g_mul(&order_minus_one, &g);
}

#[test]
fun test_g_to_bytes_regression() {
    let id = ristretto255::g_identity();
    let id_bytes = *group_ops::bytes(&id);
    let expected = x"0000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(expected, id_bytes);

    let g = ristretto255::g_generator();
    let g_bytes = *group_ops::bytes(&g);
    let expected = x"e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";
    assert_eq!(expected, g_bytes);

    let h = ristretto255::g_mul(&ristretto255::scalar_from_u64(54321), &g);
    let h_bytes = *group_ops::bytes(&h);
    let expected = x"6e94dc1325cae4b7cdbdaacf2686f0ae8fa67ec9c59fe818312f6d7eaea13e30";
    assert_eq!(expected, h_bytes);
}

#[test]
fun test_valid_g_from_bytes() {
    let g = ristretto255::g_generator();
    let g_from_bytes = ristretto255::g_from_bytes(group_ops::bytes(&g));
    assert!(group_ops::equal(&g, &g_from_bytes));

    let id = ristretto255::g_identity();
    let id_from_bytes = ristretto255::g_from_bytes(group_ops::bytes(&id));
    assert!(group_ops::equal(&id, &id_from_bytes));
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_g_too_short() {
    let _ = ristretto255::g_from_bytes(&SHORT_G_BYTES);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_g_empty() {
    let _ = ristretto255::g_from_bytes(&vector[]);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_g_too_long() {
    let _ = ristretto255::g_from_bytes(&LONG_G_BYTES);
}

#[test, expected_failure(abort_code = group_ops::EInvalidInput)]
fun test_invalid_g_div() {
    let a = ristretto255::scalar_from_u64(0);
    let b = ristretto255::g_generator();
    let _ = ristretto255::g_div(&a, &b);
}
