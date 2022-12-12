// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::bulletproofs_tests {
    use sui::bulletproofs;
    use sui::elliptic_curve as ec;

    #[test]
    fun test_ristretto_point_addition() {
        let committed_value_1 = 1000u64;
        let blinding_value_1 = 100u64;
        let committed_value_2 = 500u64;
        let blinding_value_2 = 200u64;

        let committed_sum = committed_value_1 + committed_value_2;
        let blinding_sum = blinding_value_1 + blinding_value_2;

        let point_1 = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value_1),
            ec::new_scalar_from_u64(blinding_value_1)
        );

        let point_2 = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value_2),
            ec::new_scalar_from_u64(blinding_value_2)
        );

        let point_sum_reference = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_sum),
            ec::new_scalar_from_u64(blinding_sum)
        );

        let point_sum = ec::add(&point_1, &point_2);

        assert!(ec::bytes(&point_sum) == ec::bytes(&point_sum_reference), 0)
    }

    #[test]
    fun test_ristretto_point_subtraction() {
        let committed_value_1 = 1000u64;
        let blinding_value_1 = 100u64;
        let committed_value_2 = 500u64;
        let blinding_value_2 = 50u64;

        let committed_diff = committed_value_1 - committed_value_2;
        let blinding_diff = blinding_value_1 - blinding_value_2;

        let point_1 = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value_1),
            ec::new_scalar_from_u64(blinding_value_1)
        );

        let point_2 = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value_2),
            ec::new_scalar_from_u64(blinding_value_2)
        );

        let point_diff_reference = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_diff),
            ec::new_scalar_from_u64(blinding_diff)
        );

        let point_diff = ec::subtract(&point_1, &point_2);

        assert!(ec::bytes(&point_diff) == ec::bytes(&point_diff_reference), 0)
    }

    #[test]
    fun test_pedersen_commitment() {
        // These are generated elsewhere;
        let commitment = x"e0831c2a8caaacc9f33699776a61d77b407d065d09014eba061240dbd2e17d71";

        let committed_value = 1000u64;
        let blinding_factor = 10u64;

        let point = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value),
            ec::new_scalar_from_u64(blinding_factor)
        );

        assert!(commitment == ec::bytes(&point), 0);
    }

    #[test]
    fun test_bulletproof_standard_0_2pow64_proof() {
        let bit_length: u64 = 64;

        // This has been generated in fastcrypto.
        let bulletproof = x"921bfb53ad5522ed84a1b62068edeee4bf36839051695b5c31cb0968907fe3387287ee055bf3070800e79eccee4acb5f25e8200830944ae07fda488cf066e320a69cf9f8f65373c153559fb6dd511ae7f070d29533c29fd3e83a39ae1cdf5d3f60e6894edc9c13dba28b3d93a37e9dd5039f78dc0661141a2fc3b68c9872d6400ec913bd102b26c901a6cdb8e4c546d74b1a0f3ea14b7ab44c2a612a450e69092d77442b5f4c8281e8fc51a18b2a874b4f1c17c567808406078fc532ce5a730193fee9d7bd0bee1e9564b2ef93adca4595cd83383798b555cf94da0dcdcd420b98d25d5758b4cc9221f82cd8bbb7efc2291b387d5435c6408928c740262ce956b4751bfee70d1175d7c6e269b4df25c2abbe9264620e15e680667392bcf75844808d9c5b55ad06f5686db286a90b8e74efd8b66dfa021d243f12a4248c87647eb01554b809db9a560f93e0a0c2a6c273f6bad3b16081e5a627f0b27c010a7c42fa13e0fe298e9362f629ba88e723a44d53e231d0f018b45d476f2c9bb21af30c76614cdb263ed7606955aa0b74483fe903f0fd7391ee70805dbb1c7e3e59a90758335e3ab13f07fc54fdb7ba05722615e29ca26a1727cd3d9a7e9325e0adeb28a8ee65890f0c6666342d8b65650c2d18f0700eac30585ae2b5e59c35e5504d5dc867093c4cc0a70b49342b6718981e19cdb166cb59359114187505198719ee2ad2f4f77fff907918fdfa42f572394c882b11e34f3512cc6d31ac99e27578aa2c30f85706ca26fdb1d3d538708f140a640bbc55a60059b01dc218d166a1a12e569a3ad535098db1add3aab2b7b704d5e22fcaa86fbb89430b7a6e20f66e0e65010e28dbeebac21b9136259e881506a1a74657362ca549bf80bba9a18210711e0859856040a28c743060bfec69a4e0741890ea3a708b95482d0dbb90df7a416701";

        let committed_value = 1000u64;
        let blinding_factor = 100u64;

        let point = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value),
            ec::new_scalar_from_u64(blinding_factor)
        );

        assert!(bulletproofs::verify_full_range_proof(&bulletproof, &point, bit_length) == true, 0);
    }

    #[test]
    fun test_bulletproof_standard_0_2pow64_invalid_proof() {
        let bit_length: u64 = 64;

        // This has been generated in fastcrypto and we just replaced the first byte to make it invalid.
        let bulletproof = x"001bfb53ad5522ed84a1b62068edeee4bf36839051695b5c31cb0968907fe3387287ee055bf3070800e79eccee4acb5f25e8200830944ae07fda488cf066e320a69cf9f8f65373c153559fb6dd511ae7f070d29533c29fd3e83a39ae1cdf5d3f60e6894edc9c13dba28b3d93a37e9dd5039f78dc0661141a2fc3b68c9872d6400ec913bd102b26c901a6cdb8e4c546d74b1a0f3ea14b7ab44c2a612a450e69092d77442b5f4c8281e8fc51a18b2a874b4f1c17c567808406078fc532ce5a730193fee9d7bd0bee1e9564b2ef93adca4595cd83383798b555cf94da0dcdcd420b98d25d5758b4cc9221f82cd8bbb7efc2291b387d5435c6408928c740262ce956b4751bfee70d1175d7c6e269b4df25c2abbe9264620e15e680667392bcf75844808d9c5b55ad06f5686db286a90b8e74efd8b66dfa021d243f12a4248c87647eb01554b809db9a560f93e0a0c2a6c273f6bad3b16081e5a627f0b27c010a7c42fa13e0fe298e9362f629ba88e723a44d53e231d0f018b45d476f2c9bb21af30c76614cdb263ed7606955aa0b74483fe903f0fd7391ee70805dbb1c7e3e59a90758335e3ab13f07fc54fdb7ba05722615e29ca26a1727cd3d9a7e9325e0adeb28a8ee65890f0c6666342d8b65650c2d18f0700eac30585ae2b5e59c35e5504d5dc867093c4cc0a70b49342b6718981e19cdb166cb59359114187505198719ee2ad2f4f77fff907918fdfa42f572394c882b11e34f3512cc6d31ac99e27578aa2c30f85706ca26fdb1d3d538708f140a640bbc55a60059b01dc218d166a1a12e569a3ad535098db1add3aab2b7b704d5e22fcaa86fbb89430b7a6e20f66e0e65010e28dbeebac21b9136259e881506a1a74657362ca549bf80bba9a18210711e0859856040a28c743060bfec69a4e0741890ea3a708b95482d0dbb90df7a416701";

        let committed_value = 1000u64;
        let blinding_factor = 100u64;

        let point = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value),
            ec::new_scalar_from_u64(blinding_factor)
        );

        assert!(bulletproofs::verify_full_range_proof(&bulletproof, &point, bit_length)== false, 0);
    }

}