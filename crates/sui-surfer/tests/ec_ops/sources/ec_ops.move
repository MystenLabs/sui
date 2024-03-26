// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module ec_ops::ec_ops_test {
    use sui::bls12381;
    use sui::group_ops;
    use std::vector;

    entry fun sanity(x: u32) {
        let _b = (x as u64) + 1;
    }

    entry fun scalar_basic() {
        let zero = bls12381::scalar_from_u64(0);
        let one = bls12381::scalar_from_u64(1);
        assert!(group_ops::equal(&zero, &bls12381::scalar_zero()), 0);
        assert!(group_ops::equal(&one, &bls12381::scalar_one()), 0);
        assert!(group_ops::equal(&zero, &bls12381::scalar_one()) == false, 0);
        let zero0 = bls12381::scalar_mul(&zero, &one);
        assert!(group_ops::equal(&zero0, &bls12381::scalar_zero()), 0);

        let zero_bytes = *group_ops::bytes(&zero);
        let expected = x"0000000000000000000000000000000000000000000000000000000000000000";
        assert!(expected == zero_bytes, 0);

        let eight = bls12381::scalar_from_u64(8);
        let eight_bytes = *group_ops::bytes(&eight);
        let expected = x"0000000000000000000000000000000000000000000000000000000000000008";
        assert!(expected == eight_bytes, 0);

        let minus_one = bls12381::scalar_sub(&zero, &bls12381::scalar_from_u64(1));
        let minus_one_bytes = *group_ops::bytes(&minus_one);
        let expected = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000";
        assert!(expected == minus_one_bytes, 0);

        let minus_eight = bls12381::scalar_sub(&zero, &bls12381::scalar_from_u64(8));
        let minus_eight_bytes = *group_ops::bytes(&minus_eight);
        let expected = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfefffffffefffffff9";
        assert!(expected == minus_eight_bytes, 0);
    }

    entry fun scalar_ops(x: u32, y: u32) {
        let x = (x as u64) + 1;
        let x_scalar = bls12381::scalar_from_u64(x);
        let y = (y as u64);
        let y_scalar = bls12381::scalar_from_u64(y);

        assert!(group_ops::equal(&bls12381::scalar_from_u64(x + y), &bls12381::scalar_add(&x_scalar, &y_scalar)), 0);
        let z_scalar = bls12381::scalar_sub(&x_scalar, &y_scalar);
        assert!(group_ops::equal(&bls12381::scalar_from_u64(x), &bls12381::scalar_add(&z_scalar, &y_scalar)), 0);
        assert!(group_ops::equal(&bls12381::scalar_from_u64(x * y), &bls12381::scalar_mul(&x_scalar, &y_scalar)), 0);
        let z_scalar = bls12381::scalar_div(&x_scalar, &y_scalar);
        assert!(group_ops::equal(&bls12381::scalar_from_u64(y), &bls12381::scalar_mul(&z_scalar, &x_scalar)), 0);
        let z_scalar = bls12381::scalar_neg(&x_scalar);
        assert!(group_ops::equal(&bls12381::scalar_zero(), &bls12381::scalar_add(&x_scalar, &z_scalar)), 0);
        let z_scalar = bls12381::scalar_inv(&x_scalar);
        assert!(group_ops::equal(&bls12381::scalar_one(), &bls12381::scalar_mul(&x_scalar, &z_scalar)), 0);

        let i = 0;
        let z = bls12381::scalar_add(&x_scalar, &y_scalar);
        while (i < 10) {
            let new_z = bls12381::scalar_mul(&z, &x_scalar);
            new_z = bls12381::scalar_add(&new_z, &y_scalar);
            // check back
            let rev = bls12381::scalar_sub(&new_z, &y_scalar);
            rev = bls12381::scalar_div(&x_scalar, &rev);
            assert!(group_ops::equal(&z, &rev), 0);

            let rev_as_bytes = *group_ops::bytes(&rev);
            let rev_scalar2 = bls12381::scalar_from_bytes(&rev_as_bytes);
            assert!(group_ops::equal(&rev_scalar2, &rev), 0);
            z = new_z;
            i = i + 1;
        };
    }

    entry fun g1_basic() {
        let id = bls12381::g1_identity();
        let g = bls12381::g1_generator();

        assert!(group_ops::equal(&id, &bls12381::g1_sub(&g, &g)), 0);
        assert!(group_ops::equal(&id, &bls12381::g1_sub(&id, &id)), 0);
        assert!(group_ops::equal(&g, &bls12381::g1_add(&id, &g)), 0);
        assert!(group_ops::equal(&g, &bls12381::g1_add(&g, &id)), 0);

        let two_g = bls12381::g1_add(&g, &g);
        let four_g = bls12381::g1_add(&two_g, &two_g);

        let another_four_g = bls12381::g1_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_four_g = bls12381::g1_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_id = bls12381::g1_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id), 0);

        let another_two_g = bls12381::g1_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let another_two_g = bls12381::g1_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let minus_two_g = bls12381::g1_neg(&two_g);
        let another_two_g = bls12381::g1_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let msg1 = b"123";
        let msg2 = b"321";
        let hash1 = bls12381::hash_to_g1(&msg1);
        let hash2 = bls12381::hash_to_g1(&msg2);
        let hash3 = bls12381::hash_to_g1(&msg1);
        assert!(group_ops::equal(&hash1, &hash2) == false, 0);
        assert!(group_ops::equal(&hash1, &hash3), 0);
    }

    entry fun g1_ops(x: u32, y: u32) {
        let id = bls12381::g1_identity();
        let g = bls12381::g1_generator();
        let x = (x as u64) + 1;
        let x_scalar = bls12381::scalar_from_u64(x);
        let g_x = bls12381::g1_mul(&x_scalar, &g);
        let y = (y as u64);
        let y_scalar = bls12381::scalar_from_u64(y);
        let g_y = bls12381::g1_mul(&y_scalar, &g);

        let z_g = bls12381::g1_add(&g_x, &g_y);
        let z_g2 = bls12381::g1_mul(&bls12381::scalar_from_u64(x + y), &g);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        if (x > y) {
            let z_g = bls12381::g1_sub(&g_x, &g_y);
            let z_g2 = bls12381::g1_mul(&bls12381::scalar_from_u64(x - y), &g);
            assert!(group_ops::equal(&z_g, &z_g2), 0);
        };

        let z_g = bls12381::g1_mul(&bls12381::scalar_from_u64(x * y), &g);
        let z_g2 = bls12381::g1_mul(&x_scalar, &g_y);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let z_g = bls12381::g1_div(&x_scalar, &z_g);
        assert!(group_ops::equal(&g_y, &z_g), 0);

        let z_g = bls12381::g1_neg(&g_x);
        let z_g2 = bls12381::g1_sub(&id, &g_x);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let x_as_bytes = *group_ops::bytes(&x_scalar);
        let _hash = bls12381::hash_to_g1(&x_as_bytes);

        let i = 0;
        let z = bls12381::g1_add(&g_x, &g_y);
        while (i < 10) {
            let new_z = bls12381::g1_mul(&x_scalar, &z);
            new_z = bls12381::g1_add(&new_z, &g_y);

            let rev = bls12381::g1_sub(&new_z, &g_y);
            rev = bls12381::g1_div(&x_scalar, &rev);
            assert!(group_ops::equal(&z, &rev), 0);

            let rev_as_bytes = *group_ops::bytes(&rev);
            let rev2 = bls12381::g1_from_bytes(&rev_as_bytes);
            assert!(group_ops::equal(&rev2, &rev), 0);

            z = new_z;
            x_scalar = bls12381::scalar_mul(&x_scalar, &y_scalar);
            y_scalar = bls12381::scalar_add(&y_scalar, &y_scalar);
            i = i + 1;
        }
    }


    entry fun g2_basic() {
        let id = bls12381::g2_identity();
        let g = bls12381::g2_generator();

        assert!(group_ops::equal(&id, &bls12381::g2_sub(&g, &g)), 0);
        assert!(group_ops::equal(&id, &bls12381::g2_sub(&id, &id)), 0);
        assert!(group_ops::equal(&g, &bls12381::g2_add(&id, &g)), 0);
        assert!(group_ops::equal(&g, &bls12381::g2_add(&g, &id)), 0);

        let two_g = bls12381::g2_add(&g, &g);
        let four_g = bls12381::g2_add(&two_g, &two_g);

        let another_four_g = bls12381::g2_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_four_g = bls12381::g2_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_id = bls12381::g2_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id), 0);

        let another_two_g = bls12381::g2_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let another_two_g = bls12381::g2_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let minus_two_g = bls12381::g2_neg(&two_g);
        let another_two_g = bls12381::g2_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let msg1 = b"123";
        let msg2 = b"321";
        let hash1 = bls12381::hash_to_g2(&msg1);
        let hash2 = bls12381::hash_to_g2(&msg2);
        let hash3 = bls12381::hash_to_g2(&msg1);
        assert!(group_ops::equal(&hash1, &hash2) == false, 0);
        assert!(group_ops::equal(&hash1, &hash3), 0);
    }

    entry fun g2_ops(x: u32, y: u32) {
        let id = bls12381::g2_identity();
        let g = bls12381::g2_generator();
        let x = (x as u64) + 1;
        let x_scalar = bls12381::scalar_from_u64(x);
        let g_x = bls12381::g2_mul(&x_scalar, &g);
        let y = (y as u64);
        let y_scalar = bls12381::scalar_from_u64(y);
        let g_y = bls12381::g2_mul(&y_scalar, &g);

        let z_g = bls12381::g2_add(&g_x, &g_y);
        let z_g2 = bls12381::g2_mul(&bls12381::scalar_from_u64(x + y), &g);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        if (x > y) {
            let z_g = bls12381::g2_sub(&g_x, &g_y);
            let z_g2 = bls12381::g2_mul(&bls12381::scalar_from_u64(x - y), &g);
            assert!(group_ops::equal(&z_g, &z_g2), 0);
        };

        let z_g = bls12381::g2_mul(&bls12381::scalar_from_u64(x * y), &g);
        let z_g2 = bls12381::g2_mul(&x_scalar, &g_y);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let z_g = bls12381::g2_div(&x_scalar, &z_g);
        assert!(group_ops::equal(&g_y, &z_g), 0);

        let z_g = bls12381::g2_neg(&g_x);
        let z_g2 = bls12381::g2_sub(&id, &g_x);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let x_as_bytes = *group_ops::bytes(&x_scalar);
        let _hash = bls12381::hash_to_g2(&x_as_bytes);

        let i = 0;
        let z = bls12381::g2_add(&g_x, &g_y);
        while (i < 10) {
            let new_z = bls12381::g2_mul(&x_scalar, &z);
            new_z = bls12381::g2_add(&new_z, &g_y);

            let rev = bls12381::g2_sub(&new_z, &g_y);
            rev = bls12381::g2_div(&x_scalar, &rev);
            assert!(group_ops::equal(&z, &rev), 0);

            let rev_as_bytes = *group_ops::bytes(&rev);
            let rev2 = bls12381::g2_from_bytes(&rev_as_bytes);
            assert!(group_ops::equal(&rev, &rev2), 0);

            z = new_z;
            x_scalar = bls12381::scalar_mul(&x_scalar, &y_scalar);
            y_scalar = bls12381::scalar_add(&y_scalar, &y_scalar);
            i = i + 1;
        }
    }


    entry fun gt_basic() {
        let id = bls12381::gt_identity();
        let g = bls12381::gt_generator();

        assert!(group_ops::equal(&id, &bls12381::gt_sub(&g, &g)), 0);
        assert!(group_ops::equal(&id, &bls12381::gt_sub(&id, &id)), 0);
        assert!(group_ops::equal(&g, &bls12381::gt_add(&id, &g)), 0);
        assert!(group_ops::equal(&g, &bls12381::gt_add(&g, &id)), 0);

        let two_g = bls12381::gt_add(&g, &g);
        let four_g = bls12381::gt_add(&two_g, &two_g);

        let another_four_g = bls12381::gt_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_four_g = bls12381::gt_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g), 0);

        let another_id = bls12381::gt_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id), 0);

        let another_two_g = bls12381::gt_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let another_two_g = bls12381::gt_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);

        let minus_two_g = bls12381::gt_neg(&two_g);
        let another_two_g = bls12381::gt_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g), 0);
    }

    entry fun gt_ops(x: u32, y: u32) {
        let id = bls12381::gt_identity();
        let g = bls12381::gt_generator();
        let x = (x as u64) + 1;
        let x_scalar = bls12381::scalar_from_u64(x);
        let g_x = bls12381::gt_mul(&x_scalar, &g);
        let y = (y as u64);
        let y_scalar = bls12381::scalar_from_u64(y);
        let g_y = bls12381::gt_mul(&y_scalar, &g);

        let z_g = bls12381::gt_add(&g_x, &g_y);
        let z_g2 = bls12381::gt_mul(&bls12381::scalar_from_u64(x + y), &g);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        if (x > y) {
            let z_g = bls12381::gt_sub(&g_x, &g_y);
            let z_g2 = bls12381::gt_mul(&bls12381::scalar_from_u64(x - y), &g);
            assert!(group_ops::equal(&z_g, &z_g2), 0);
        };

        let z_g = bls12381::gt_mul(&bls12381::scalar_from_u64(x * y), &g);
        let z_g2 = bls12381::gt_mul(&x_scalar, &g_y);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let z_g = bls12381::gt_div(&x_scalar, &z_g);
        assert!(group_ops::equal(&g_y, &z_g), 0);

        let z_g = bls12381::gt_neg(&g_x);
        let z_g2 = bls12381::gt_sub(&id, &g_x);
        assert!(group_ops::equal(&z_g, &z_g2), 0);

        let _x_as_bytes = *group_ops::bytes(&g_x);

        let i = 0;
        let z = bls12381::gt_add(&g_x, &g_y);
        while (i < 10) {
            let new_z = bls12381::gt_mul(&x_scalar, &z);
            new_z = bls12381::gt_add(&new_z, &g_y);

            let rev = bls12381::gt_sub(&new_z, &g_y);
            rev = bls12381::gt_div(&x_scalar, &rev);
            assert!(group_ops::equal(&z, &rev), 0);
            z = new_z;
            x_scalar = bls12381::scalar_mul(&x_scalar, &y_scalar);
            y_scalar = bls12381::scalar_add(&y_scalar, &y_scalar);
            i = i + 1;
        }
    }

    entry fun pairing(x: u32, y: u32) {
        let g1 = bls12381::g1_generator();
        let g2 = bls12381::g2_generator();
        let gt = bls12381::gt_generator();
        assert!(group_ops::equal(&bls12381::pairing(&g1, &g2), &gt), 0);

        let g1_3 = bls12381::g1_mul(&bls12381::scalar_from_u64(3), &g1);
        let g2_5 = bls12381::g2_mul(&bls12381::scalar_from_u64(5), &g2);
        let gt_5 = bls12381::gt_mul(&bls12381::scalar_from_u64(15), &gt);
        assert!(group_ops::equal(&bls12381::pairing(&g1_3, &g2_5), &gt_5), 0);

        assert!(
            group_ops::equal(
                &bls12381::pairing(&bls12381::g1_identity(), &bls12381::g2_identity()),
                &bls12381::gt_identity()
            ),
            0
        );
        assert!(
            group_ops::equal(
                &bls12381::pairing(&bls12381::g1_generator(), &bls12381::g2_identity()),
                &bls12381::gt_identity()
            ),
            0
        );
        assert!(
            group_ops::equal(
                &bls12381::pairing(&bls12381::g1_identity(), &bls12381::g2_generator()),
                &bls12381::gt_identity()
            ),
            0
        );

        let x = (x as u64);
        let x_scalar = bls12381::scalar_from_u64(x);
        let g_x = bls12381::g1_mul(&x_scalar, &g1);
        let y = (y as u64);
        let y_scalar = bls12381::scalar_from_u64(y);
        let g_y = bls12381::g2_mul(&y_scalar, &g2);
        let gt_xy = bls12381::gt_mul(&bls12381::scalar_from_u64(x * y), &gt);
        assert!(group_ops::equal(&bls12381::pairing(&g_x, &g_y), &gt_xy), 0);

        let i = 0;
        while (i < 10) {
            g_x = bls12381::g1_mul(&x_scalar, &g_x);
            g_y = bls12381::g2_mul(&y_scalar, &g_y);
            let _res = bls12381::pairing(&g_x, &g_y);
            i = i + 1;
        }
    }

    entry fun g1_msm(x: u32) {
        let i = 1;
        let expected_result = bls12381::g1_identity();
        let g = bls12381::g1_generator();
        let scalars: vector<group_ops::Element<bls12381::Scalar>> = vector::empty();
        let elements: vector<group_ops::Element<bls12381::G1>> = vector::empty();
        while (i < 20) {
            let base_scalar = bls12381::scalar_from_u64(i);
            let base = bls12381::g1_mul(&base_scalar, &g);
            let exponent_scalar = bls12381::scalar_from_u64(i);
            let base_exp = bls12381::g1_mul(&exponent_scalar, &base);
            vector::push_back(&mut elements, base);
            vector::push_back(&mut scalars, exponent_scalar);
            expected_result = bls12381::g1_add(&expected_result, &base_exp);
            i = i + 1;
        };

        let result = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result), 11);
    }
}
