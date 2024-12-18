// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(implicit_const_copy)]
#[test_only]
module sui::bls12381_tests {
    use sui::bls12381;
    use sui::group_ops;
    use sui::random;
    use sui::test_utils::assert_eq;
    use std::hash::sha2_256;

    const ORDER_BYTES: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";
    const ORDER_MINUS_ONE_BYTES: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000";
    const LONG_SCALAR_BYTES: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff0000000000";
    const SHORT_SCALAR_BYTES: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff0000";
    const LONG_G1_BYTES: vector<u8> = x"97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bbbbbb";
    const SHORT_G1_BYTES: vector<u8> = x"97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb";
    const LONG_G2_BYTES: vector<u8> = x"93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb811";
    const SHORT_G2_BYTES: vector<u8> = x"93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121";

    #[test]
    fun test_bls12381_min_sig_valid_sig() {
        let msg = x"0101010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == true)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_sig() {
        let msg = x"0201010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_signature_key_length() {
        let msg = x"0201010101";
        let pk = x"606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e34002e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_public_key_length() {
        let msg = x"0201010101";
        let pk = x"606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false)
    }

    #[test]
    fun test_bls12381_min_pk_valid_and_invalid_sig() {
        // Test an actual Drand response.
        let pk = x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"a2cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == true);
        // Check invalid signatures.
        let invalid_sig = x"11118577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        assert!(verify_drand_round(pk, invalid_sig, prev_sig, round) == false);
        assert!(verify_drand_round(pk, sig, prev_sig, round + 1) == false);
    }

    #[test]
    fun test_bls12381_min_pk_invalid_signature_key_length() {
        let pk = x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == false);
    }

    #[test]
    fun test_bls12381_min_pk_invalid_public_key_length() {
        let pk = x"8f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"a2cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == false);
    }

    fun drand_message(mut prev_sig: vector<u8>, mut round: u64): vector<u8> {
        // The signed message can be computed in Rust using:
        //  let mut sha = Sha256::new();
        //  sha.update(&prev_sig);
        //  sha.update(round.to_be_bytes());
        //  let digest = sha.finalize().digest;
        let mut round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let mut i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = &mut round_bytes[i];
            *curr_element = curr_byte as u8;
            round = round >> 8;
            i = i - 1;
        };
        prev_sig.append(round_bytes);
        sha2_256(prev_sig)
    }

    fun verify_drand_round(pk: vector<u8>, sig: vector<u8>, prev_sig: vector<u8>, round: u64): bool {
        let msg = drand_message(prev_sig, round);
        bls12381::bls12381_min_pk_verify(&sig, &pk, &msg)
    }


    //// Group operations ////

    #[test]
    fun test_scalar_ops() {
        let zero = bls12381::scalar_from_u64(0);
        let one = bls12381::scalar_from_u64(1);
        assert!(group_ops::equal(&zero, &bls12381::scalar_zero()));
        assert!(group_ops::equal(&one, &bls12381::scalar_one()));
        assert!(group_ops::equal(&zero, &bls12381::scalar_one()) == false);
        let zero0 = bls12381::scalar_mul(&zero, &one);
        assert!(group_ops::equal(&zero0, &bls12381::scalar_zero()));

        let two = bls12381::scalar_add(&one, &one);
        let four = bls12381::scalar_add(&two, &two);
        assert!(group_ops::equal(&four, &bls12381::scalar_from_u64(4)));

        let eight = bls12381::scalar_mul(&four, &two);
        assert!(group_ops::equal(&eight, &bls12381::scalar_from_u64(8)));

        let zero0 = bls12381::scalar_mul(&zero, &eight);
        assert!(group_ops::equal(&zero0, &bls12381::scalar_zero()));

        let eight2 = bls12381::scalar_mul(&eight, &one);
        assert!(group_ops::equal(&eight2, &bls12381::scalar_from_u64(8)));

        let six = bls12381::scalar_sub(&eight, &two);
        assert!(group_ops::equal(&six, &bls12381::scalar_from_u64(6)));

        let minus_six = bls12381::scalar_sub(&two, &eight);
        let three = bls12381::scalar_add(&minus_six, &bls12381::scalar_from_u64(9));
        assert!(group_ops::equal(&three, &bls12381::scalar_from_u64(3)));

        let three = bls12381::scalar_div(&two, &six);
        assert!(group_ops::equal(&three, &bls12381::scalar_from_u64(3)));

        let minus_three = bls12381::scalar_neg(&three);
        assert!(group_ops::equal(&bls12381::scalar_add(&minus_three, &six), &bls12381::scalar_from_u64(3)));

        let minus_zero = bls12381::scalar_neg(&zero);
        assert!(group_ops::equal(&minus_zero, &zero));

        let inv_three = bls12381::scalar_inv(&three);
        assert!(group_ops::equal(&bls12381::scalar_mul(&six, &inv_three), &bls12381::scalar_from_u64(2)));

        let order_minus_one = bls12381::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = bls12381::scalar_add(&order_minus_one, &order_minus_one);
        let _ = bls12381::scalar_mul(&order_minus_one, &order_minus_one);
    }

    #[test]
    fun test_scalar_more_ops() {
        let mut gen = random::new_generator_for_testing();
        let x = gen.generate_u32() as u64;
        let x_scalar = bls12381::scalar_from_u64(x);
        let y = gen.generate_u32() as u64;
        let y_scalar = bls12381::scalar_from_u64(y);

        // Since x, y are u32 numbers, the following operations do not overflow as u64.
        assert!(group_ops::equal(&bls12381::scalar_from_u64(x + y), &bls12381::scalar_add(&x_scalar, &y_scalar)));
        let z_scalar = bls12381::scalar_sub(&x_scalar, &y_scalar);
        assert!(group_ops::equal(&bls12381::scalar_from_u64(x), &bls12381::scalar_add(&z_scalar, &y_scalar)));
        assert!(group_ops::equal(&bls12381::scalar_from_u64(x * y), &bls12381::scalar_mul(&x_scalar, &y_scalar)));
        let z_scalar = bls12381::scalar_div(&x_scalar, &y_scalar);
        assert!(group_ops::equal(&bls12381::scalar_from_u64(y), &bls12381::scalar_mul(&z_scalar, &x_scalar)));
        let z_scalar = bls12381::scalar_neg(&x_scalar);
        assert!(group_ops::equal(&bls12381::scalar_zero(), &bls12381::scalar_add(&x_scalar, &z_scalar)));
        let z_scalar = bls12381::scalar_inv(&x_scalar);
        assert!(group_ops::equal(&bls12381::scalar_one(), &bls12381::scalar_mul(&x_scalar, &z_scalar)));

        let mut i = 0;
        let mut z = bls12381::scalar_add(&x_scalar, &y_scalar);
        while (i < 20) {
            let mut new_z = bls12381::scalar_mul(&z, &x_scalar);
            new_z = bls12381::scalar_add(&new_z, &y_scalar);
            // check back
            let mut rev = bls12381::scalar_sub(&new_z, &y_scalar);
            rev = bls12381::scalar_div(&x_scalar, &rev);
            assert!(group_ops::equal(&z, &rev));

            let rev_as_bytes = *group_ops::bytes(&rev);
            let rev_scalar2 = bls12381::scalar_from_bytes(&rev_as_bytes);
            assert!(group_ops::equal(&rev_scalar2, &rev));
            z = new_z;
            i = i + 1;
        };
    }

    #[test]
    fun test_scalar_to_bytes_regression() {
        let zero = bls12381::scalar_from_u64(0);
        let zero_bytes = *group_ops::bytes(&zero);
        let expected = x"0000000000000000000000000000000000000000000000000000000000000000";
        assert_eq(expected, zero_bytes);

        let eight = bls12381::scalar_from_u64(8);
        let eight_bytes = *group_ops::bytes(&eight);
        let expected = x"0000000000000000000000000000000000000000000000000000000000000008";
        assert_eq(expected, eight_bytes);

        let minus_one = bls12381::scalar_sub(&zero, &bls12381::scalar_from_u64(1));
        let minus_one_bytes = *group_ops::bytes(&minus_one);
        let expected = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000";
        assert_eq(expected, minus_one_bytes);

        let minus_eight = bls12381::scalar_sub(&zero, &bls12381::scalar_from_u64(8));
        let minus_eight_bytes = *group_ops::bytes(&minus_eight);
        let expected = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfefffffffefffffff9";
        assert_eq(expected, minus_eight_bytes);
    }

    #[test]
    fun test_valid_scalar_from_bytes() {
        let eight = bls12381::scalar_from_u64(8);
        let eight_from_bytes = bls12381::scalar_from_bytes(group_ops::bytes(&eight));
        assert!(group_ops::equal(&eight, &eight_from_bytes));

        let zero = bls12381::scalar_zero();
        let zero_from_bytes = bls12381::scalar_from_bytes(group_ops::bytes(&zero));
        assert!(group_ops::equal(&zero, &zero_from_bytes));
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_order() {
        let _ = bls12381::scalar_from_bytes(&ORDER_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_empty() {
        let _ = bls12381::scalar_from_bytes(&vector[]);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_too_short() {
        let _ = bls12381::scalar_from_bytes(&SHORT_SCALAR_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_too_long() {
        let _ = bls12381::scalar_from_bytes(&LONG_SCALAR_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_div() {
        let a = bls12381::scalar_from_u64(0);
        let b = bls12381::scalar_from_u64(10);
        let _ = bls12381::scalar_div(&a, &b);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_scalar_inv() {
        let a = bls12381::scalar_from_u64(0);
        let _ = bls12381::scalar_inv(&a);
    }


    #[test]
    fun test_g1_ops() {
        let id = bls12381::g1_identity();
        let g = bls12381::g1_generator();

        assert!(group_ops::equal(&id, &bls12381::g1_sub(&g, &g)));
        assert!(group_ops::equal(&id, &bls12381::g1_sub(&id, &id)));
        assert!(group_ops::equal(&g, &bls12381::g1_add(&id, &g)));
        assert!(group_ops::equal(&g, &bls12381::g1_add(&g, &id)));

        let two_g = bls12381::g1_add(&g, &g);
        let four_g = bls12381::g1_add(&two_g, &two_g);

        let another_four_g = bls12381::g1_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_four_g = bls12381::g1_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_id = bls12381::g1_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id));

        let another_two_g = bls12381::g1_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let another_two_g = bls12381::g1_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let minus_two_g = bls12381::g1_neg(&two_g);
        let another_two_g = bls12381::g1_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let order_minus_one = bls12381::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = bls12381::g1_mul(&order_minus_one, &g);

        let msg1 = b"123";
        let msg2 = b"321";
        let hash1 = bls12381::hash_to_g1(&msg1);
        let hash2 = bls12381::hash_to_g1(&msg2);
        let hash3 = bls12381::hash_to_g1(&msg1);
        assert!(group_ops::equal(&hash1, &hash2) == false);
        assert!(group_ops::equal(&hash1, &hash3));
    }

    #[test]
    fun test_g1_to_bytes_regression() {
        let id = bls12381::g1_identity();
        let id_bytes = *group_ops::bytes(&id);
        let expected = x"c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        assert_eq(expected, id_bytes);

        let g = bls12381::g1_generator();
        let g_bytes = *group_ops::bytes(&g);
        let expected = x"97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb";
        assert_eq(expected, g_bytes);

        let h = bls12381::g1_mul(&bls12381::scalar_from_u64(54321), &g);
        let h_bytes = *group_ops::bytes(&h);
        let expected = x"821285b97f9c0420a2d37951edbda3d7c3ebac40c6f194faa0256f6e569eba49829cd69c27f1dd9df2dd83bac1f5aa49";
        assert_eq(expected, h_bytes);
    }

    #[test]
    fun test_valid_g1_from_bytes() {
        let g = bls12381::g1_generator();
        let g_from_bytes = bls12381::g1_from_bytes(group_ops::bytes(&g));
        assert!(group_ops::equal(&g, &g_from_bytes));

        let id = bls12381::g1_identity();
        let id_from_bytes = bls12381::g1_from_bytes(group_ops::bytes(&id));
        assert!(group_ops::equal(&id, &id_from_bytes));
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g1_too_short() {
        let _ = bls12381::g1_from_bytes(&SHORT_G1_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g1_empty() {
        let _ = bls12381::g1_from_bytes(&vector[]);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g1_too_long() {
        let _ = bls12381::g1_from_bytes(&LONG_G1_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g1_div() {
        let a = bls12381::scalar_from_u64(0);
        let b = bls12381::g1_generator();
        let _ = bls12381::g1_div(&a, &b);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g1_empty_msg() {
        let _ = bls12381::hash_to_g1(&vector[]);
    }

    #[random_test]
    fun test_to_from_uncompressed_g1(scalar: u64) {
        // Generator
        let a = bls12381::g1_generator();
        let a_uncompressed = bls12381::g1_to_uncompressed_g1(&a);
        assert!(a_uncompressed.bytes() == x"17f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb08b3f481e3aaa0f1a09e30ed741d8ae4fcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1");
        let reconstructed = bls12381::uncompressed_g1_to_g1(&a_uncompressed);
        assert!(group_ops::equal(&a, &reconstructed));

        // Identity element
        let b = bls12381::g1_identity();
        let b_uncompressed = bls12381::g1_to_uncompressed_g1(&b);
        assert!(b_uncompressed.bytes() == x"400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
        let reconstructed = bls12381::uncompressed_g1_to_g1(&b_uncompressed);
        assert!(group_ops::equal(&b, &reconstructed));

        // Random element
        let scalar = bls12381::scalar_from_u64(scalar);
        let c = bls12381::g1_mul(&scalar, &bls12381::g1_generator());
        let c_uncompressed = bls12381::g1_to_uncompressed_g1(&c);
        let reconstructed = bls12381::uncompressed_g1_to_g1(&c_uncompressed);
        assert!(group_ops::equal(&c, &reconstructed));
    }

    #[test]
    fun test_uncompressed_g1_sum() {
        // Empty sum
        let sum = bls12381::uncompressed_g1_sum(&vector[]);
        assert!(group_ops::equal(&bls12381::g1_to_uncompressed_g1(&bls12381::g1_identity()), &sum));

        // Sum with random terms
        let mut gen = random::new_generator_for_testing();
        let mut elements = vector[];
        let mut i = 100;
        let mut expected_result = bls12381::g1_identity();
        while (i > 0) {
            let scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let element = bls12381::g1_mul(&scalar, &bls12381::g1_generator());
            expected_result = bls12381::g1_add(&expected_result, &element);
            let uncompressed_element = bls12381::g1_to_uncompressed_g1(&element);
            elements.push_back(uncompressed_element);
            let actual_result = bls12381::uncompressed_g1_sum(&elements);
            assert!(group_ops::equal(&bls12381::g1_to_uncompressed_g1(&expected_result), &actual_result));
            i = i - 1;
        };
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInputTooLong)]
    fun test_uncompressed_g1_sum_too_long() {
        // Sum with random terms
        let mut gen = random::new_generator_for_testing();
        let mut elements = vector[];
        let mut i = 1201;
        while (i > 0) {
            let scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let element = bls12381::g1_mul(&scalar, &bls12381::g1_generator());
            let uncompressed_element = bls12381::g1_to_uncompressed_g1(&element);
            elements.push_back(uncompressed_element);
            i = i - 1;
        };
        let _ = bls12381::uncompressed_g1_sum(&elements);
    }

    #[test]
    fun test_g2_ops() {
        let id = bls12381::g2_identity();
        let g = bls12381::g2_generator();

        assert!(group_ops::equal(&id, &bls12381::g2_sub(&g, &g)));
        assert!(group_ops::equal(&id, &bls12381::g2_sub(&id, &id)));
        assert!(group_ops::equal(&g, &bls12381::g2_add(&id, &g)));
        assert!(group_ops::equal(&g, &bls12381::g2_add(&g, &id)));

        let two_g = bls12381::g2_add(&g, &g);
        let four_g = bls12381::g2_add(&two_g, &two_g);

        let another_four_g = bls12381::g2_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_four_g = bls12381::g2_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_id = bls12381::g2_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id));

        let another_two_g = bls12381::g2_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let another_two_g = bls12381::g2_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let minus_two_g = bls12381::g2_neg(&two_g);
        let another_two_g = bls12381::g2_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let order_minus_one = bls12381::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = bls12381::g2_mul(&order_minus_one, &g);

        let msg1 = b"123";
        let msg2 = b"321";
        let hash1 = bls12381::hash_to_g2(&msg1);
        let hash2 = bls12381::hash_to_g2(&msg2);
        let hash3 = bls12381::hash_to_g2(&msg1);
        assert!(group_ops::equal(&hash1, &hash2) == false);
        assert!(group_ops::equal(&hash1, &hash3));
    }

    #[test]
    fun test_g2_to_bytes_regression() {
        let id = bls12381::g2_identity();
        let id_bytes = *group_ops::bytes(&id);
        let expected = x"c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        assert_eq(expected, id_bytes);

        let g = bls12381::g2_generator();
        let g_bytes = *group_ops::bytes(&g);
        let expected = x"93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8";
        assert_eq(expected, g_bytes);

        let h = bls12381::g2_mul(&bls12381::scalar_from_u64(54321), &g);
        let h_bytes = *group_ops::bytes(&h);
        let expected = x"827f4d489fcad0bf484adcc739dfe0dfbe5c0a999fd8aa022e1b66d199f328a5a2ff6ee1e568f09dc39df4c44f9527bc011cb2a633188bd08146f6a730de30f8e5cdcaa48d69e27dd945315f5a3950bc311be5a5ea9f3d4a4cdd4315ae65040a";
        assert_eq(expected, h_bytes);
    }

    #[test]
    fun test_valid_g2_from_bytes() {
        let g = bls12381::g2_generator();
        let g_from_bytes = bls12381::g2_from_bytes(group_ops::bytes(&g));
        assert!(group_ops::equal(&g, &g_from_bytes));

        let id = bls12381::g2_identity();
        let id_from_bytes = bls12381::g2_from_bytes(group_ops::bytes(&id));
        assert!(group_ops::equal(&id, &id_from_bytes));
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g2_empty() {
        let _ = bls12381::g2_from_bytes(&vector[]);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g2_too_short() {
        let _ = bls12381::g2_from_bytes(&SHORT_G2_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g2_too_long() {
        let _ = bls12381::g2_from_bytes(&LONG_G2_BYTES);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g2_div() {
        let a = bls12381::scalar_from_u64(0);
        let b = bls12381::g2_generator();
        let _ = bls12381::g2_div(&a, &b);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_g2_empty_msg() {
        let _ = bls12381::hash_to_g2(&vector[]);
    }


    #[test]
    fun test_gt_ops() {
        let id = bls12381::gt_identity();
        let g = bls12381::gt_generator();

        assert!(group_ops::equal(&id, &bls12381::gt_sub(&g, &g)));
        assert!(group_ops::equal(&id, &bls12381::gt_sub(&id, &id)));
        assert!(group_ops::equal(&g, &bls12381::gt_add(&id, &g)));
        assert!(group_ops::equal(&g, &bls12381::gt_add(&g, &id)));

        let two_g = bls12381::gt_add(&g, &g);
        let four_g = bls12381::gt_add(&two_g, &two_g);

        let another_four_g = bls12381::gt_add(&id, &four_g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_four_g = bls12381::gt_mul(&bls12381::scalar_from_u64(4), &g);
        assert!(group_ops::equal(&four_g, &another_four_g));

        let another_id = bls12381::gt_mul(&bls12381::scalar_from_u64(0), &g);
        assert!(group_ops::equal(&id, &another_id));

        let another_two_g = bls12381::gt_sub(&four_g, &two_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let another_two_g = bls12381::gt_div(&bls12381::scalar_from_u64(2), &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let minus_two_g = bls12381::gt_neg(&two_g);
        let another_two_g = bls12381::gt_add(&minus_two_g, &four_g);
        assert!(group_ops::equal(&two_g, &another_two_g));

        let order_minus_one = bls12381::scalar_from_bytes(&ORDER_MINUS_ONE_BYTES);
        let _ = bls12381::gt_mul(&order_minus_one, &g);
    }

    #[test]
    fun test_gt_to_bytes_regression() {
        let id = bls12381::gt_identity();
        let id_bytes = *group_ops::bytes(&id);
        let expected = x"000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
        assert_eq(expected, id_bytes);

        let g = bls12381::gt_generator();
        let g_bytes = *group_ops::bytes(&g);
        let expected = x"1250ebd871fc0a92a7b2d83168d0d727272d441befa15c503dd8e90ce98db3e7b6d194f60839c508a84305aaca1789b6089a1c5b46e5110b86750ec6a532348868a84045483c92b7af5af689452eafabf1a8943e50439f1d59882a98eaa0170f19f26337d205fb469cd6bd15c3d5a04dc88784fbb3d0b2dbdea54d43b2b73f2cbb12d58386a8703e0f948226e47ee89d06fba23eb7c5af0d9f80940ca771b6ffd5857baaf222eb95a7d2809d61bfe02e1bfd1b68ff02f0b8102ae1c2d5d5ab1a1368bb445c7c2d209703f239689ce34c0378a68e72a6b3b216da0e22a5031b54ddff57309396b38c881c4c849ec23e87193502b86edb8857c273fa075a50512937e0794e1e65a7617c90d8bd66065b1fffe51d7a579973b1315021ec3c19934f11b8b424cd48bf38fcef68083b0b0ec5c81a93b330ee1a677d0d15ff7b984e8978ef48881e32fac91b93b47333e2ba5703350f55a7aefcd3c31b4fcb6ce5771cc6a0e9786ab5973320c806ad360829107ba810c5a09ffdd9be2291a0c25a99a201b2f522473d171391125ba84dc4007cfbf2f8da752f7c74185203fcca589ac719c34dffbbaad8431dad1c1fb597aaa5018107154f25a764bd3c79937a45b84546da634b8f6be14a8061e55cceba478b23f7dacaa35c8ca78beae9624045b4b604c581234d086a9902249b64728ffd21a189e87935a954051c7cdba7b3872629a4fafc05066245cb9108f0242d0fe3ef0f41e58663bf08cf068672cbd01a7ec73baca4d72ca93544deff686bfd6df543d48eaa24afe47e1efde449383b676631";
        assert_eq(expected, g_bytes);

        let h = bls12381::gt_mul(&bls12381::scalar_from_u64(54321), &g);
        let h_bytes = *group_ops::bytes(&h);
        let expected = x"19763b7ba7f964f0266ea4acb45211c2348c42c557e02568cd9e43f23f6dfab7fff3b8ee79ff9c0ff36c8368e82f92ff08f9578afa437e97d2c515232f8acc83a562afe5dba1ce39d0a12cdd7df45ba52139291ff44192df325e6a3be19049920eff6c9895f58f39a102b930509a66047c2e77126de80d2fababcf71f0b83ff95fb2e5ce51b99ffb02262a2845158b53095654819a67adbf8c992c565df26b1d363f9044ca515b5d33f27f2f6848a9fb9d55ae13a5690a47d3e8e03c8441d506018083311e3531c1a3a935331913875452ec92656947e6ab101d9ef2cd358ca04d083eadc6159b1da86e49a4c11d9b731213e4113b956ddcf6d7d87dd70f7f55fc08c5935618aca8e8621ae9d4de2d06dda448c842b6954f90ba5fb31bb3ff64167384dd48635008693bac01aa0863ff4bf763126de8f9e19d6dac221e64539aacbae77f1938d46bd71735eb5c1730fd19be89613cda8e59ff5e2446adea9659322a14e668d5d13b36bb0548125b8f0d357936990b47a876a8a22c1a33cdf92509052d4c632440d161db2e807d953fa330ac7b10333b97ec4aba2bbcb9fdd43d62f17d7d0db194bce39f3ea78bd53501095d790bacc492a9ddf499aaec722f922bd2f1d491d53c979cabeea22a3be374674c6257e543ae8a1e574425a58d96e90b2caa4fd13534158dd916fc8682858013cb343d367db7d3cdc8cae8efa8241ce175a6b328b57b6f180e850ec53a6a6808fc45b2acaf06492a4a1fe98fd71d31b643bd8ef9472f0f83eeb5cd8b63a371c7c91891d0d637f9345024e8673802e2";
        assert_eq(expected, h_bytes);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_invalid_gt_div() {
        let a = bls12381::scalar_from_u64(0);
        let b = bls12381::gt_generator();
        let _ = bls12381::gt_div(&a, &b);
    }

    #[test]
    fun test_msm_g1() {
        let mut expected_result = bls12381::g1_identity();
        let g = bls12381::g1_generator();
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let mut elements: vector<group_ops::Element<bls12381::G1>> = vector[];
        let mut gen = random::new_generator_for_testing();
        let mut i = gen.generate_u8() % 32 + 1;
        while (i > 0) {
            let base_scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let base = bls12381::g1_mul(&base_scalar, &g);
            let exponent_scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let base_exp = bls12381::g1_mul(&exponent_scalar, &base);
            elements.push_back(base);
            scalars.push_back(exponent_scalar);
            expected_result = bls12381::g1_add(&expected_result, &base_exp);
            i = i - 1;
        };
        let result = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result));
    }

    #[test]
    fun test_msm_g1_edge_cases() {
        let zero = bls12381::scalar_zero();
        let one = bls12381::scalar_one();
        let g = bls12381::g1_generator();
        let id = bls12381::g1_identity();
        let mut gen = random::new_generator_for_testing();
        let r = bls12381::scalar_from_u64(gen.generate_u32() as u64);
        let g_r = bls12381::g1_mul(&r, &g);

        let result = bls12381::g1_multi_scalar_multiplication(&vector[zero], &vector[g]);
        assert!(group_ops::equal(&result, &id));

        let result = bls12381::g1_multi_scalar_multiplication(&vector[one], &vector[g]);
        assert!(group_ops::equal(&result, &g));

        let result = bls12381::g1_multi_scalar_multiplication(&vector[one, one], &vector[g, id]);
        assert!(group_ops::equal(&result, &g));

        let result = bls12381::g1_multi_scalar_multiplication(&vector[zero, one], &vector[g, id]);
        assert!(group_ops::equal(&result, &id));

        let result = bls12381::g1_multi_scalar_multiplication(&vector[one, one], &vector[g_r, id]);
        assert!(group_ops::equal(&result, &g_r));
    }

    #[test]
    fun test_msm_g1_id() {
        let mut i = 1;
        let expected_result = bls12381::g1_identity();
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let mut elements: vector<group_ops::Element<bls12381::G1>> = vector[];
        while (i < 33) {
            let scalar = bls12381::scalar_from_u64(i);
            scalars.push_back(scalar);
            elements.push_back(bls12381::g1_identity());
            i = i + 1;
        };
        let result = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result));
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_empty_g1_msm() {
        let scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let elements: vector<group_ops::Element<bls12381::G1>> = vector[];
        let _ = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_diff_length_g1_msm() {
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        scalars.push_back(bls12381::scalar_zero());
        scalars.push_back(bls12381::scalar_one());
        let mut elements: vector<group_ops::Element<bls12381::G1>> = vector[];
        elements.push_back(bls12381::g1_generator());
        let _ = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInputTooLong)]
    fun test_msm_g1_too_long() {
        let mut i = 1;
        let mut expected_result = bls12381::g1_identity();
        let g = bls12381::g1_generator();
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let mut elements: vector<group_ops::Element<bls12381::G1>> = vector[];
        while (i < 34) {
            // this limit is defined in the protocol config
            let base_scalar = bls12381::scalar_from_u64(i);
            let base = bls12381::g1_mul(&base_scalar, &g);
            let exponent_scalar = bls12381::scalar_from_u64(i + 100);
            let base_exp = bls12381::g1_mul(&exponent_scalar, &base);
            elements.push_back(base);
            scalars.push_back(exponent_scalar);
            expected_result = bls12381::g1_add(&expected_result, &base_exp);
            i = i + 1;
        };
        let result = bls12381::g1_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result));
    }

    #[test]
    fun test_msm_g2() {
        let mut expected_result = bls12381::g2_identity();
        let g = bls12381::g2_generator();
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let mut elements: vector<group_ops::Element<bls12381::G2>> = vector[];
        let mut gen = random::new_generator_for_testing();
        let mut i = gen.generate_u8() % 32 + 1;
        while (i > 0) {
            let base_scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let base = bls12381::g2_mul(&base_scalar, &g);
            let exponent_scalar = bls12381::scalar_from_u64(gen.generate_u64());
            let base_exp = bls12381::g2_mul(&exponent_scalar, &base);
            elements.push_back(base);
            scalars.push_back(exponent_scalar);
            expected_result = bls12381::g2_add(&expected_result, &base_exp);
            i = i - 1;
        };
        let result = bls12381::g2_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result));
    }

    #[test]
    fun test_msm_g2_edge_cases() {
        let zero = bls12381::scalar_zero();
        let one = bls12381::scalar_one();
        let g = bls12381::g2_generator();
        let id = bls12381::g2_identity();
        let mut gen = random::new_generator_for_testing();
        let r = bls12381::scalar_from_u64(gen.generate_u32() as u64);
        let g_r = bls12381::g2_mul(&r, &g);

        let result = bls12381::g2_multi_scalar_multiplication(&vector[zero], &vector[g]);
        assert!(group_ops::equal(&result, &id));

        let result = bls12381::g2_multi_scalar_multiplication(&vector[one], &vector[g]);
        assert!(group_ops::equal(&result, &g));

        let result = bls12381::g2_multi_scalar_multiplication(&vector[one, one], &vector[g, id]);
        assert!(group_ops::equal(&result, &g));

        let result = bls12381::g2_multi_scalar_multiplication(&vector[zero, one], &vector[g, id]);
        assert!(group_ops::equal(&result, &id));

        let result = bls12381::g2_multi_scalar_multiplication(&vector[one, one], &vector[g_r, id]);
        assert!(group_ops::equal(&result, &g_r));
    }

    #[test]
    fun test_msm_g2_id() {
        let mut i = 1;
        let expected_result = bls12381::g2_identity();
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let mut elements: vector<group_ops::Element<bls12381::G2>> = vector[];
        while (i < 20) {
            let scalar = bls12381::scalar_from_u64(i);
            scalars.push_back(scalar);
            elements.push_back(bls12381::g2_identity());
            i = i + 1;
        };
        let result = bls12381::g2_multi_scalar_multiplication(&scalars, &elements);
        assert!(group_ops::equal(&result, &expected_result));
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_empty_g2_msm() {
        let scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        let elements: vector<group_ops::Element<bls12381::G2>> = vector[];
        let _ = bls12381::g2_multi_scalar_multiplication(&scalars, &elements);
    }

    #[test]
    #[expected_failure(abort_code = group_ops::EInvalidInput)]
    fun test_diff_length_g2_msm() {
        let mut scalars: vector<group_ops::Element<bls12381::Scalar>> = vector[];
        scalars.push_back(bls12381::scalar_zero());
        scalars.push_back(bls12381::scalar_one());
        let mut elements: vector<group_ops::Element<bls12381::G2>> = vector[];
        elements.push_back(bls12381::g2_generator());
        let _ = bls12381::g2_multi_scalar_multiplication(&scalars, &elements);
    }

    #[test]
    fun test_pairing() {
        let g1 = bls12381::g1_generator();
        let g2 = bls12381::g2_generator();
        let gt = bls12381::gt_generator();
        assert_eq(bls12381::pairing(&g1, &g2), gt);

        let g1_3 = bls12381::g1_mul(&bls12381::scalar_from_u64(3), &g1);
        let g2_5 = bls12381::g2_mul(&bls12381::scalar_from_u64(5), &g2);
        let gt_5 = bls12381::gt_mul(&bls12381::scalar_from_u64(15), &gt);
        assert_eq(bls12381::pairing(&g1_3, &g2_5), gt_5);

        assert_eq(bls12381::pairing(&bls12381::g1_identity(), &bls12381::g2_identity()), bls12381::gt_identity());
        assert_eq(bls12381::pairing(&bls12381::g1_generator(), &bls12381::g2_identity()), bls12381::gt_identity());
        assert_eq(bls12381::pairing(&bls12381::g1_identity(), &bls12381::g2_generator()), bls12381::gt_identity());
    }

    #[test]
    fun test_group_ops_with_standard_signatures_min_sig() {
        let msg = x"0101010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let pk = bls12381::g2_from_bytes(&pk);
        let sig = bls12381::g1_from_bytes(&sig);
        let hashed_msg = bls12381::hash_to_g1(&msg);

        let pairing1 = bls12381::pairing(&sig, &bls12381::g2_generator());
        let pairing2 = bls12381::pairing(&hashed_msg, &pk);
        assert_eq(pairing1, pairing2);
    }

    #[test]
    fun test_group_ops_with_standard_signatures_min_pk() {
        // Using the above drand outputs.
        let pk = x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"a2cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";

        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        let msg = drand_message(prev_sig, round);

        let pk = bls12381::g1_from_bytes(&pk);
        let sig = bls12381::g2_from_bytes(&sig);
        let hashed_msg = bls12381::hash_to_g2(&msg);

        let pairing1 = bls12381::pairing(&bls12381::g1_generator(), &sig);
        let pairing2 = bls12381::pairing(&pk, &hashed_msg);
        assert_eq(pairing1, pairing2);
    }

    // TODO: test vectors?
}
