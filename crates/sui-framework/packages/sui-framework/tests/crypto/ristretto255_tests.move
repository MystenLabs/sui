// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::ristretto_tests {
    use sui::ristretto255;
    use sui::group_ops;
    use std::vector;

    const ORDER_BYTES: vector<u8> = x"edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    const ORDER_MINUS_ONE_BYTES: vector<u8> = x"ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    const LONG_SCALAR_BYTES: vector<u8> = x"1010ecd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    const SHORT_SCALAR_BYTES: vector<u8> = x"f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";
    const LONG_G_BYTES: vector<u8> = x"1010e2f2ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";
    const SHORT_G_BYTES: vector<u8> = x"ae0a6abc4e71a884a961c500515f58e30b6aa582dd8db6a65945e08d2d76";

    #[test]
    fun test_scalar_ops() {
        let zero = ristretto255::scalar_from_u64(0);
        let one = ristretto255::scalar_from_u64(1);

        assert!(group_ops::equal(&zero, &ristretto255::scalar_zero()), 0);
        assert!(group_ops::equal(&one, &ristretto255::scalar_one()), 0);
        assert!(group_ops::equal(&zero, &ristretto255::scalar_one()) == false, 0);
        assert!(group_ops::equal(&zero, &ristretto255::scalar_sub(&one, &one)), 0);

        let two = ristretto255::scalar_add(&one, &one);
        let four = ristretto255::scalar_add(&two, &two);
        assert!(group_ops::equal(&four, &ristretto255::scalar_from_u64(4)), 0);

        let eight = ristretto255::scalar_mul(&four, &two);
        assert!(group_ops::equal(&eight, &ristretto255::scalar_from_u64(8)), 0);

        let six = ristretto255::scalar_sub(&eight, &two);
        assert!(group_ops::equal(&six, &ristretto255::scalar_from_u64(6)), 0);

        let three = ristretto255::scalar_div(&two, &six);
        assert!(group_ops::equal(&three, &ristretto255::scalar_from_u64(3)), 0);

        let minus_three = ristretto255::scalar_neg(&three);
        assert!(group_ops::equal(&ristretto255::scalar_add(&minus_three, &six), &ristretto255::scalar_from_u64(3)), 0);

        let inv_three = ristretto255::scalar_inv(&three);
        assert!(group_ops::equal(&ristretto255::scalar_mul(&six, &inv_three), &ristretto255::scalar_from_u64(2)), 0);

        let order_minus_one = ristretto255::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = ristretto255::scalar_add(&order_minus_one, &order_minus_one);
        let _ = ristretto255::scalar_mul(&order_minus_one, &order_minus_one);
    }


    #[test]
    fun test_valid_scalar_from_bytes() {
        let eight = ristretto255::scalar_from_u64(8);
        let eight_from_bytes = ristretto255::scalar_from_bytes(group_ops::bytes(&eight));
        assert!(group_ops::equal(&eight, &eight_from_bytes), 0);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_order() {
        let _ = ristretto255::scalar_from_bytes(&ORDER_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_too_short() {
        let _ = ristretto255::scalar_from_bytes(&SHORT_SCALAR_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_too_long() {
        let _ = ristretto255::scalar_from_bytes(&LONG_SCALAR_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_div() {
        let a = ristretto255::scalar_from_u64(0);
        let b = ristretto255::scalar_from_u64(10);
        let _ = ristretto255::scalar_div(&a, &b);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_inv() {
        let a = ristretto255::scalar_from_u64(0);
        let _ = ristretto255::scalar_inv(&a);
    }

    #[test]
    fun test_g_ops() {
        let id = ristretto255::g_identity();
        let g = ristretto255::g_generator();

        assert!(group_ops::equal(&id, &ristretto255::g_sub(&g, &g)), 0);

        let two_g = ristretto255::g_add(&g, &g);
        let four_g = ristretto255::g_add(&two_g, &two_g);

        let another_four_g = ristretto255::g_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_four_g = ristretto255::g_mul(&ristretto255::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_two_g = ristretto255::g_div(&ristretto255::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let another_two_g = ristretto255::g_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let minus_two_g = ristretto255::g_neg(&two_g);
        let another_two_g = ristretto255::g_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let order_minus_one = ristretto255::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = ristretto255::g_mul(&order_minus_one, &g);

        // hash_to
        let msg1 = b"123";
        let msg2 = b"321";
        let hash1 = ristretto255::hash_to_g(&msg1);
        let hash2 = ristretto255::hash_to_g(&msg2);
        assert!(group_ops::equal(&hash1, &hash2) == false, 0);
    }

    #[test]
    fun test_valid_gt_from_bytes() {
        let g = ristretto255::g_generator();
        let g_from_bytes = ristretto255::g_from_bytes(group_ops::bytes(&g));
        assert!(group_ops::equal(&g, &g_from_bytes), 0);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g_too_short() {
        let _ = ristretto255::g_from_bytes(&SHORT_G_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g_too_long() {
        let _ = ristretto255::g_from_bytes(&LONG_G_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g_div() {
        let a = ristretto255::scalar_from_u64(0);
        let b = ristretto255::g_generator();
        let _ = ristretto255::g_div(&a, &b);
    }

    #[test]
    fun test_msm() {
        let i = 1;
        let expected_result = ristretto255::g_identity();
        let g = ristretto255::g_generator();
        let scalars: vector<group_ops::Element<ristretto255::Scalar>> = vector::empty();
        let elements: vector<group_ops::Element<ristretto255::G>> = vector::empty();
        while (i < 20) {
            let scalar = ristretto255::scalar_from_u64(i);
            vector::push_back(&mut elements, g);
            let g = ristretto255::g_mul(&scalar, &g);
            vector::push_back(&mut scalars, scalar);
            expected_result = ristretto255::g_add(&expected_result, &g);
            i = i + 1;
        };
        let result = ristretto255::g_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result), 0);
    }

    #[test]
    fun test_regression() {
        // Based on the RFC
        let rfc_four_g = x"da80862773358b466ffadfe0b3293ab3d9fd53c5ea6c955358f568322daf6a57";
        let four_g = ristretto255::g_mul(&ristretto255::scalar_from_u64(4), &ristretto255::g_generator());
        assert!(rfc_four_g == *group_ops::bytes(&four_g), 0);

        // TODO: more test vectors
    }
}
