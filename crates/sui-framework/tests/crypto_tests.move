// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::crypto_tests {
    use sui::ecdsa;
    use sui::bls12381;
    use sui::bulletproofs;
    use sui::digest;
    use sui::elliptic_curve as ec;
    use sui::hmac;
    use std::vector;
    use std::hash::sha2_256;
    use sui::groth16;

    #[test]
    fun test_ecrecover_pubkey() {
        // test case generated against https://docs.rs/secp256k1/latest/secp256k1/
        let hashed_msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30,
        146, 162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let sig = vector[132, 220, 128, 67, 151, 154, 45, 143, 50, 56, 176, 134, 137, 58, 223, 166, 191, 230, 178, 184,
        123, 11, 19, 69, 59, 205, 72, 206, 153, 187, 184, 7, 16, 74, 73, 45, 38, 238, 81, 96, 138, 225, 235, 143, 95,
        142, 185, 56, 99, 3, 97, 27, 66, 99, 79, 225, 139, 21, 67, 254, 78, 251, 176, 176, 0];

        let pubkey_bytes = vector[2, 2, 87, 224, 47, 124, 255, 117, 223, 91, 188, 190, 151, 23, 241, 173, 148, 107, 20,
        103, 63, 155, 108, 151, 251, 152, 205, 205, 239, 71, 224, 86, 9];

        let pubkey = ecdsa::ecrecover(&sig, &hashed_msg);
        assert!(pubkey == pubkey_bytes, 0);
    }

    #[test]
    fun test_ecrecover_pubkey_2() {
        // Test case from go-ethereum: https://github.com/ethereum/go-ethereum/blob/master/crypto/signature_test.go#L37
        // hashed_msg: 0xce0677bb30baa8cf067c88db9811f4333d131bf8bcf12fe7065d211dce971008
        // sig: 0x90f27b8b488db00b00606796d2987f6a5f59ae62ea05effe84fef5b8b0e549984a691139ad57a3f0b906637673aa2f63d1f55cb1a69199d4009eea23ceaddc9301
        // pubkey: 0x02e32df42865e97135acfb65f3bae71bdc86f4d49150ad6a440b6f15878109880a
        let hashed_msg = vector[206, 6, 119, 187, 48, 186, 168, 207, 6, 124, 136, 219, 152, 17, 244, 51, 61, 19, 27,
        248, 188, 241, 47, 231, 6, 93, 33, 29, 206, 151, 16, 8];
        let sig = vector[144, 242, 123, 139, 72, 141, 176, 11, 0, 96, 103, 150, 210, 152, 127, 106, 95, 89, 174, 98,
        234, 5, 239, 254, 132, 254, 245, 184, 176, 229, 73, 152, 74, 105, 17, 57, 173, 87, 163, 240, 185, 6, 99, 118,
        115, 170, 47, 99, 209, 245, 92, 177, 166, 145, 153, 212, 0, 158, 234, 35, 206, 173, 220, 147, 1];
        let pubkey_bytes = vector[2, 227, 45, 244, 40, 101, 233, 113, 53, 172, 251, 101, 243, 186, 231, 27, 220, 134,
        244, 212, 145, 80, 173, 106, 68, 11, 111, 21, 135, 129, 9, 136, 10];

        let pubkey = ecdsa::ecrecover(&sig, &hashed_msg);
        assert!(pubkey == pubkey_bytes, 0);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa::EFailToRecoverPubKey)]
    fun test_ecrecover_pubkey_fail_to_recover() {
        let hashed_msg = vector[0];
        let sig = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        ecdsa::ecrecover(&sig, &hashed_msg);
    }

    #[test]
    #[expected_failure(abort_code = ecdsa::EInvalidSignature)]
    fun test_ecrecover_pubkey_invalid_sig() {
        let hashed_msg = vector[0];
        // incorrect length sig
        let sig = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        ecdsa::ecrecover(&sig, &hashed_msg);
    }

    #[test]
    fun test_keccak256_hash() {
        let msg = b"hello world!";
        let hashed_msg_bytes = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175,
        30, 146, 162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let hashed_msg = ecdsa::keccak256(&msg);
        assert!(hashed_msg == hashed_msg_bytes, 0);
    }

    #[test]
    fun test_hmac_sha3_256() {
        let key = b"my key!";
        let msg = b"hello world!";
        // The next was calculated using python
        // hmac.new(key, msg, digestmod=hashlib.sha3_256).digest()
        let expected_output_bytes = vector[246, 214, 174, 2, 244, 38, 235, 150, 100, 232, 158, 60, 109, 134, 198, 14, 97, 3, 206, 34, 185, 22, 129, 146, 25, 194, 110, 52, 232, 210, 54, 220];
        let output = hmac::hmac_sha3_256(&key, &msg);
        let outout_bytes = digest::sha3_256_digest_to_bytes(&output);
        assert!(outout_bytes == expected_output_bytes, 0);
    }

    #[test]
    fun test_bls12381_min_sig_valid_sig() {
        let msg = vector[1, 1, 1, 1, 1];

        let pk = vector[141, 241, 1, 96, 111, 145, 243, 202, 215, 245, 75, 138, 255, 15, 15, 100, 196, 28, 72, 45, 155,
        159, 159, 232, 29, 43, 96, 123, 197, 246, 17, 189, 250, 128, 23, 207, 4, 180, 123, 68, 178, 34, 195, 86, 239,
        85, 95, 189, 17, 5, 140, 82, 192, 119, 245, 167, 236, 106, 21, 204, 253, 99, 159, 220, 155, 212, 125, 0, 90, 17,
        29, 214, 205, 184, 192, 47, 228, 150, 8, 223, 85, 163, 201, 130, 41, 134, 173, 11, 134, 189, 234, 58, 191, 223,
        228, 100];

        let sig = vector[144, 142, 52, 95, 46, 40, 3, 205, 148, 26, 232, 140, 33, 140, 150, 25, 66, 51, 201, 5, 63, 161,
        188, 165, 33, 36, 120, 125, 60, 202, 20, 28, 54, 66, 157, 118, 82, 67, 90, 130, 12, 114, 153, 45, 94, 238, 99,
        23];

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == true, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_sig() {
        let msg = vector[2, 1, 1, 1, 1];

        let pk = vector[141, 241, 1, 96, 111, 145, 243, 202, 215, 245, 75, 138, 255, 15, 15, 100, 196, 28, 72, 45, 155,
        159, 159, 232, 29, 43, 96, 123, 197, 246, 17, 189, 250, 128, 23, 207, 4, 180, 123, 68, 178, 34, 195, 86, 239,
        85, 95, 189, 17, 5, 140, 82, 192, 119, 245, 167, 236, 106, 21, 204, 253, 99, 159, 220, 155, 212, 125, 0, 90, 17,
        29, 214, 205, 184, 192, 47, 228, 150, 8, 223, 85, 163, 201, 130, 41, 134, 173, 11, 134, 189, 234, 58, 191, 223,
        228, 100];

        let sig = vector[144, 142, 52, 95, 46, 40, 3, 205, 148, 26, 232, 140, 33, 140, 150, 25, 66, 51, 201, 5, 63, 161,
        188, 165, 33, 36, 120, 125, 60, 202, 20, 28, 54, 66, 157, 118, 82, 67, 90, 130, 12, 114, 153, 45, 94, 238, 99,
        23];

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_signature_key_length() {
        let msg = vector[2, 1, 1, 1, 1];

        let pk = vector[96, 111, 145, 243, 202, 215, 245, 75, 138, 255, 15, 15, 100, 196, 28, 72, 45, 155, 159, 159,
        232, 29, 43, 96, 123, 197, 246, 17, 189, 250, 128, 23, 207, 4, 180, 123, 68, 178, 34, 195, 86, 239, 85, 95, 189,
        17, 5, 140, 82, 192, 119, 245, 167, 236, 106, 21, 204, 253, 99, 159, 220, 155, 212, 125, 0, 90, 17, 29, 214,
        205, 184, 192, 47, 228, 150, 8, 223, 85, 163, 201, 130, 41, 134, 173, 11, 134, 189, 234, 58, 191, 223, 228,
        100];

        let sig = vector[144, 142, 52, 0, 46, 40, 3, 205, 148, 26, 232, 140, 33, 140, 150, 25, 66, 51, 201, 5, 63, 161,
        188, 165, 33, 36, 120, 125, 60, 202, 20, 28, 54, 66, 157, 118, 82, 67, 90, 130, 12, 114, 153, 45, 94, 238, 99,
        23];

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_public_key_length() {
        let msg = vector[2, 1, 1, 1, 1];

        let pk = vector[96, 111, 145, 243, 202, 215, 245, 75, 138, 255, 15, 15, 100, 196, 28, 72, 45, 155, 159, 159,
        232, 29, 43, 96, 123, 197, 246, 17, 189, 250, 128, 23, 207, 4, 180, 123, 68, 178, 34, 195, 86, 239, 85, 95, 189,
        17, 5, 140, 82, 192, 119, 245, 167, 236, 106, 21, 204, 253, 99, 159, 220, 155, 212, 125, 0, 90, 17, 29, 214,
        205, 184, 192, 47, 228, 150, 8, 223, 85, 163, 201, 130, 41, 134, 173, 11, 134, 189, 234, 58, 191, 223, 228,
        100];

        let sig = vector[144, 142, 52, 95, 46, 40, 3, 205, 148, 26, 232, 140, 33, 140, 150, 25, 66, 51, 201, 5, 63, 161,
        188, 165, 33, 36, 120, 125, 60, 202, 20, 28, 54, 66, 157, 118, 82, 67, 90, 130, 12, 114, 153, 45, 94, 238, 99,
        23];

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    fun verify_drand_round(pk: vector<u8>, sig: vector<u8>, prev_sig: vector<u8>, round: u64): bool {
        // The signed message can be computed in Rust using:
        //  let mut sha = Sha256::new();
        //  sha.update(&prev_sig);
        //  sha.update(round.to_be_bytes());
        //  let digest = sha.finalize().digest;
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };
        vector::append(&mut prev_sig, round_bytes);
        let digest = sha2_256(prev_sig);
        bls12381::bls12381_min_pk_verify(&sig, &pk, &digest)
    }

    #[test]
    fun test_bls12381_min_pk_valid_and_invalid_sig() {
        // Test an actual Drand response.
        let pk = x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"a2cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == true, 0);
        // Check invalid signatures.
        let invalid_sig = x"11118577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        assert!(verify_drand_round(pk, invalid_sig, prev_sig, round) == false, 0);
        assert!(verify_drand_round(pk, sig, prev_sig, round + 1) == false, 0);
    }

    #[test]
    fun test_bls12381_min_pk_invalid_signature_key_length() {
        let pk = x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == false, 0);
    }

    #[test]
    fun test_bls12381_min_pk_invalid_public_key_length() {
        let pk = x"8f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        let sig = x"a2cd8577944b84484ef557a7f92f0d5092779497cc470b1b97680b8f7c807d97250d310b801c7c2185c7c8a21032d45403b97530ca87bd8f05d0cf4ffceb4bcb9bf7184fb604967db7e9e6ea555bc51b25a9e41fbd51181f712aa73aaec749fe";
        let prev_sig = x"a96aace596906562dc525dba4dff734642d71b334d51324f9c9bcb5a3d6caf14b05cde91d6507bf4615cb4285e5b4efd1358ebc46b80b51e338f9dc46cca17cf2e046765ba857c04101a560887fa81aef101a5bb3b2350884558bd3adc72be37";
        let round: u64 = 2373935;
        assert!(verify_drand_round(pk, sig, prev_sig, round) == false, 0);
    }

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
        let commitment = vector[224, 131, 28, 42, 140, 170, 172, 201, 243, 54, 153, 119, 106, 97, 215, 123, 64, 125, 6,
        93, 9, 1, 78, 186, 6, 18, 64, 219, 210, 225, 125, 113];

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
        let bulletproof = vector[146, 27, 251, 83, 173, 85, 34, 237, 132, 161, 182, 32, 104, 237, 238, 228, 191, 54,
        131, 144, 81, 105, 91, 92, 49, 203, 9, 104, 144, 127, 227, 56, 114, 135, 238, 5, 91, 243, 7, 8, 0, 231, 158,
        204, 238, 74, 203, 95, 37, 232, 32, 8, 48, 148, 74, 224, 127, 218, 72, 140, 240, 102, 227, 32, 166, 156, 249,
        248, 246, 83, 115, 193, 83, 85, 159, 182, 221, 81, 26, 231, 240, 112, 210, 149, 51, 194, 159, 211, 232, 58, 57,
        174, 28, 223, 93, 63, 96, 230, 137, 78, 220, 156, 19, 219, 162, 139, 61, 147, 163, 126, 157, 213, 3, 159, 120,
        220, 6, 97, 20, 26, 47, 195, 182, 140, 152, 114, 214, 64, 14, 201, 19, 189, 16, 43, 38, 201, 1, 166, 205, 184,
        228, 197, 70, 215, 75, 26, 15, 62, 161, 75, 122, 180, 76, 42, 97, 42, 69, 14, 105, 9, 45, 119, 68, 43, 95, 76,
        130, 129, 232, 252, 81, 161, 139, 42, 135, 75, 79, 28, 23, 197, 103, 128, 132, 6, 7, 143, 197, 50, 206, 90, 115,
        1, 147, 254, 233, 215, 189, 11, 238, 30, 149, 100, 178, 239, 147, 173, 202, 69, 149, 205, 131, 56, 55, 152, 181,
        85, 207, 148, 218, 13, 205, 205, 66, 11, 152, 210, 93, 87, 88, 180, 204, 146, 33, 248, 44, 216, 187, 183, 239,
        194, 41, 27, 56, 125, 84, 53, 198, 64, 137, 40, 199, 64, 38, 44, 233, 86, 180, 117, 27, 254, 231, 13, 17, 117,
        215, 198, 226, 105, 180, 223, 37, 194, 171, 190, 146, 100, 98, 14, 21, 230, 128, 102, 115, 146, 188, 247, 88,
        68, 128, 141, 156, 91, 85, 173, 6, 245, 104, 109, 178, 134, 169, 11, 142, 116, 239, 216, 182, 109, 250, 2, 29,
        36, 63, 18, 164, 36, 140, 135, 100, 126, 176, 21, 84, 184, 9, 219, 154, 86, 15, 147, 224, 160, 194, 166, 194,
        115, 246, 186, 211, 177, 96, 129, 229, 166, 39, 240, 178, 124, 1, 10, 124, 66, 250, 19, 224, 254, 41, 142, 147,
        98, 246, 41, 186, 136, 231, 35, 164, 77, 83, 226, 49, 208, 240, 24, 180, 93, 71, 111, 44, 155, 178, 26, 243, 12,
        118, 97, 76, 219, 38, 62, 215, 96, 105, 85, 170, 11, 116, 72, 63, 233, 3, 240, 253, 115, 145, 238, 112, 128, 93,
        187, 28, 126, 62, 89, 169, 7, 88, 51, 94, 58, 177, 63, 7, 252, 84, 253, 183, 186, 5, 114, 38, 21, 226, 156, 162,
        106, 23, 39, 205, 61, 154, 126, 147, 37, 224, 173, 235, 40, 168, 238, 101, 137, 15, 12, 102, 102, 52, 45, 139,
        101, 101, 12, 45, 24, 240, 112, 14, 172, 48, 88, 90, 226, 181, 229, 156, 53, 229, 80, 77, 93, 200, 103, 9, 60,
        76, 192, 167, 11, 73, 52, 43, 103, 24, 152, 30, 25, 205, 177, 102, 203, 89, 53, 145, 20, 24, 117, 5, 25, 135,
        25, 238, 42, 210, 244, 247, 127, 255, 144, 121, 24, 253, 250, 66, 245, 114, 57, 76, 136, 43, 17, 227, 79, 53,
        18, 204, 109, 49, 172, 153, 226, 117, 120, 170, 44, 48, 248, 87, 6, 202, 38, 253, 177, 211, 213, 56, 112, 143,
        20, 10, 100, 11, 188, 85, 166, 0, 89, 176, 29, 194, 24, 209, 102, 161, 161, 46, 86, 154, 58, 213, 53, 9, 141,
        177, 173, 211, 170, 178, 183, 183, 4, 213, 226, 47, 202, 168, 111, 187, 137, 67, 11, 122, 110, 32, 246, 110, 14,
        101, 1, 14, 40, 219, 238, 186, 194, 27, 145, 54, 37, 158, 136, 21, 6, 161, 167, 70, 87, 54, 44, 165, 73, 191,
        128, 187, 169, 161, 130, 16, 113, 30, 8, 89, 133, 96, 64, 162, 140, 116, 48, 96, 191, 236, 105, 164, 224, 116,
        24, 144, 234, 58, 112, 139, 149, 72, 45, 13, 187, 144, 223, 122, 65, 103, 1];

        let committed_value = 1000u64;
        let blinding_factor = 100u64;

        let point = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value),
            ec::new_scalar_from_u64(blinding_factor)
        );

        bulletproofs::verify_full_range_proof(&bulletproof, &point, bit_length);
    }

    #[test]
    #[expected_failure(abort_code = bulletproofs::EBulletproofsVerificationFailed)]
    fun test_bulletproof_standard_0_2pow64_invalid_proof() {
        let bit_length: u64 = 64;

        // This has been generated in fastcrypto and we just replaced the first byte to make it invalid.
        let bulletproof = vector[0, 27, 251, 83, 173, 85, 34, 237, 132, 161, 182, 32, 104, 237, 238, 228, 191, 54,
        131, 144, 81, 105, 91, 92, 49, 203, 9, 104, 144, 127, 227, 56, 114, 135, 238, 5, 91, 243, 7, 8, 0, 231, 158,
        204, 238, 74, 203, 95, 37, 232, 32, 8, 48, 148, 74, 224, 127, 218, 72, 140, 240, 102, 227, 32, 166, 156, 249,
        248, 246, 83, 115, 193, 83, 85, 159, 182, 221, 81, 26, 231, 240, 112, 210, 149, 51, 194, 159, 211, 232, 58, 57,
        174, 28, 223, 93, 63, 96, 230, 137, 78, 220, 156, 19, 219, 162, 139, 61, 147, 163, 126, 157, 213, 3, 159, 120,
        220, 6, 97, 20, 26, 47, 195, 182, 140, 152, 114, 214, 64, 14, 201, 19, 189, 16, 43, 38, 201, 1, 166, 205, 184,
        228, 197, 70, 215, 75, 26, 15, 62, 161, 75, 122, 180, 76, 42, 97, 42, 69, 14, 105, 9, 45, 119, 68, 43, 95, 76,
        130, 129, 232, 252, 81, 161, 139, 42, 135, 75, 79, 28, 23, 197, 103, 128, 132, 6, 7, 143, 197, 50, 206, 90, 115,
        1, 147, 254, 233, 215, 189, 11, 238, 30, 149, 100, 178, 239, 147, 173, 202, 69, 149, 205, 131, 56, 55, 152, 181,
        85, 207, 148, 218, 13, 205, 205, 66, 11, 152, 210, 93, 87, 88, 180, 204, 146, 33, 248, 44, 216, 187, 183, 239,
        194, 41, 27, 56, 125, 84, 53, 198, 64, 137, 40, 199, 64, 38, 44, 233, 86, 180, 117, 27, 254, 231, 13, 17, 117,
        215, 198, 226, 105, 180, 223, 37, 194, 171, 190, 146, 100, 98, 14, 21, 230, 128, 102, 115, 146, 188, 247, 88,
        68, 128, 141, 156, 91, 85, 173, 6, 245, 104, 109, 178, 134, 169, 11, 142, 116, 239, 216, 182, 109, 250, 2, 29,
        36, 63, 18, 164, 36, 140, 135, 100, 126, 176, 21, 84, 184, 9, 219, 154, 86, 15, 147, 224, 160, 194, 166, 194,
        115, 246, 186, 211, 177, 96, 129, 229, 166, 39, 240, 178, 124, 1, 10, 124, 66, 250, 19, 224, 254, 41, 142, 147,
        98, 246, 41, 186, 136, 231, 35, 164, 77, 83, 226, 49, 208, 240, 24, 180, 93, 71, 111, 44, 155, 178, 26, 243, 12,
        118, 97, 76, 219, 38, 62, 215, 96, 105, 85, 170, 11, 116, 72, 63, 233, 3, 240, 253, 115, 145, 238, 112, 128, 93,
        187, 28, 126, 62, 89, 169, 7, 88, 51, 94, 58, 177, 63, 7, 252, 84, 253, 183, 186, 5, 114, 38, 21, 226, 156, 162,
        106, 23, 39, 205, 61, 154, 126, 147, 37, 224, 173, 235, 40, 168, 238, 101, 137, 15, 12, 102, 102, 52, 45, 139,
        101, 101, 12, 45, 24, 240, 112, 14, 172, 48, 88, 90, 226, 181, 229, 156, 53, 229, 80, 77, 93, 200, 103, 9, 60,
        76, 192, 167, 11, 73, 52, 43, 103, 24, 152, 30, 25, 205, 177, 102, 203, 89, 53, 145, 20, 24, 117, 5, 25, 135,
        25, 238, 42, 210, 244, 247, 127, 255, 144, 121, 24, 253, 250, 66, 245, 114, 57, 76, 136, 43, 17, 227, 79, 53,
        18, 204, 109, 49, 172, 153, 226, 117, 120, 170, 44, 48, 248, 87, 6, 202, 38, 253, 177, 211, 213, 56, 112, 143,
        20, 10, 100, 11, 188, 85, 166, 0, 89, 176, 29, 194, 24, 209, 102, 161, 161, 46, 86, 154, 58, 213, 53, 9, 141,
        177, 173, 211, 170, 178, 183, 183, 4, 213, 226, 47, 202, 168, 111, 187, 137, 67, 11, 122, 110, 32, 246, 110, 14,
        101, 1, 14, 40, 219, 238, 186, 194, 27, 145, 54, 37, 158, 136, 21, 6, 161, 167, 70, 87, 54, 44, 165, 73, 191,
        128, 187, 169, 161, 130, 16, 113, 30, 8, 89, 133, 96, 64, 162, 140, 116, 48, 96, 191, 236, 105, 164, 224, 116,
        24, 144, 234, 58, 112, 139, 149, 72, 45, 13, 187, 144, 223, 122, 65, 103, 1];

        let committed_value = 1000u64;
        let blinding_factor = 100u64;

        let point = ec::create_pedersen_commitment(
            ec::new_scalar_from_u64(committed_value),
            ec::new_scalar_from_u64(blinding_factor)
        );

        bulletproofs::verify_full_range_proof(&bulletproof, &point, bit_length);
    }

    #[test]
    fun test_secp256k1_valid_sig() {
        let msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146,
        162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let pk = vector[2, 121, 190, 102, 126, 249, 220, 187, 172, 85, 160, 98, 149, 206, 135, 11, 7, 2, 155, 252, 219,
        45, 206, 40, 217, 89, 242, 129, 91, 22, 248, 23, 152];

        let sig = vector[156, 122, 114, 255, 30, 125, 177, 100, 107, 159, 148, 67, 203, 26, 53, 99, 170, 58, 99, 68,
        228, 229, 19, 239, 185, 98, 88, 199, 103, 106, 196, 137, 89, 83, 98, 157, 64, 154, 131, 36, 114, 183, 16, 160,
        40, 40, 93, 254, 196, 115, 58, 44, 27, 176, 162, 116, 158, 70, 90, 24, 41, 43, 139, 214, 1];

        let verify = ecdsa::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == true, 0)
    }

    #[test]
    fun test_secp256k1_invalid_sig() {
        let msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146,
        162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let pk = vector[2, 121, 190, 102, 126, 249, 220, 187, 172, 85, 160, 98, 149, 206, 135, 11, 7, 2, 155, 252, 219,
        45, 206, 40, 217, 89, 242, 129, 91, 22, 248, 23, 152];

        // sig in the form of (r, s, 0) instead of (r, s, 1)
        let sig = vector[156, 122, 114, 255, 30, 125, 177, 100, 107, 159, 148, 67, 203, 26, 53, 99, 170, 58, 99, 68,
        228, 229, 19, 239, 185, 98, 88, 199, 103, 106, 196, 137, 89, 83, 98, 157, 64, 154, 131, 36, 114, 183, 16, 160,
        40, 40, 93, 254, 196, 115, 58, 44, 27, 176, 162, 116, 158, 70, 90, 24, 41, 43, 139, 214, 0];

        let verify = ecdsa::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_sig_length() {
        let msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146,
        162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let pk = vector[2, 121, 190, 102, 126, 249, 220, 187, 172, 85, 160, 98, 149, 206, 135, 11, 7, 2, 155, 252, 219,
        45, 206, 40, 217, 89, 242, 129, 91, 22, 248, 23, 152];
        let sig = vector[156, 122, 114, 255, 30, 125, 177, 100, 107, 159, 148, 67, 203, 26, 53, 99, 170, 58, 99, 68,
        228, 229, 19, 239, 185, 98, 88, 199, 103, 106, 196, 137, 89, 83, 98, 157, 64, 154, 131, 36, 114, 183, 16, 160,
        40, 40, 93, 254, 196, 115, 58, 44, 27, 176, 162, 116, 158, 70, 90, 24, 41, 43, 139, 214];
        let verify = ecdsa::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_hashed_msg_length() {
        let msg = vector[1];

        let pk = vector[2, 121, 190, 102, 126, 249, 220, 187, 172, 85, 160, 98, 149, 206, 135, 11, 7, 2, 155, 252, 219,
        45, 206, 40, 217, 89, 242, 129, 91, 22, 248, 23, 152];

        let sig = vector[156, 122, 114, 255, 30, 125, 177, 100, 107, 159, 148, 67, 203, 26, 53, 99, 170, 58, 99, 68,
        228, 229, 19, 239, 185, 98, 88, 199, 103, 106, 196, 137, 89, 83, 98, 157, 64, 154, 131, 36, 114, 183, 16, 160,
        40, 40, 93, 254, 196, 115, 58, 44, 27, 176, 162, 116, 158, 70, 90, 24, 41, 43, 139, 214];

        let verify = ecdsa::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_secp256k1_invalid_public_key_length() {
        let msg = vector[87, 202, 161, 118, 175, 26, 192, 67, 60, 93, 243, 14, 141, 171, 205, 46, 193, 175, 30, 146,
        162, 110, 206, 213, 247, 25, 184, 132, 88, 119, 124, 214];

        let pk = vector[121, 190, 102, 126, 249, 220, 187, 172, 85, 160, 98, 149, 206, 135, 11, 7, 2, 155, 252, 219, 45,
        206, 40, 217, 89, 242, 129, 91, 22, 248, 23, 152];

        let sig = vector[156, 122, 114, 255, 30, 125, 177, 100, 107, 159, 148, 67, 203, 26, 53, 99, 170, 58, 99, 68,
        228, 229, 19, 239, 185, 98, 88, 199, 103, 106, 196, 137, 89, 83, 98, 157, 64, 154, 131, 36, 114, 183, 16, 160,
        40, 40, 93, 254, 196, 115, 58, 44, 27, 176, 162, 116, 158, 70, 90, 24, 41, 43, 139, 214, 1];

        let verify = ecdsa::secp256k1_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_ecrecover_eth_address() {
        // Due to the lack of conversion tool in Move, here we convert hex to vector in python3: [x for x in bytearray.fromhex(hex_string[2:])]
        // Test case from https://web3js.readthedocs.io/en/v1.7.5/web3-eth-accounts.html#recover
        // signature: 0xb91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c
        // hashed_msg: 0x1da44b586eb0729ff70a73c326926f6ed5a25f5b056e7f47fbc6e58d86871655
        // address: 0x2c7536e3605d9c16a7a3d7b1898e529396a65c23
        let sig = vector[185, 20, 103, 229, 112, 166, 70, 106, 169, 233, 135, 108, 188, 208, 19, 186, 186, 2, 144, 11,
        137, 121, 212, 63, 226, 8, 164, 164, 243, 57, 245, 253, 96, 7, 231, 76, 216, 46, 3, 123, 128, 1, 134, 66, 47,
        194, 218, 22, 124, 116, 126, 240, 69, 229, 209, 138, 95, 93, 67, 0, 248, 225, 160, 41, 28];

        let hashed_msg = vector[29, 164, 75, 88, 110, 176, 114, 159, 247, 10, 115, 195, 38, 146, 111, 110, 213, 162, 95,
        91, 5, 110, 127, 71, 251, 198, 229, 141, 134, 135, 22, 85];

        let addr1 = vector[44, 117, 54, 227, 96, 93, 156, 22, 167, 163, 215, 177, 137, 142, 82, 147, 150, 166, 92, 35];

        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);

        // Test case from https://etherscan.io/verifySig/9754
        // sig: 0xcb614cba67d6a37b9cb90d21635d81ed035b8ccb99f0befe05495b819111119b17ecf0c0cb4bcc781de387206f6dfcd9f1b99e1b54b44c376412d8f5c919b1981b
        // hashed_msg: 0x1da44b586eb0729ff70a73c326926f6ed5a25f5b056e7f47fbc6e58d86871655
        // addr: 0x4cbf668fca6f10d01f161122534044436b80702e
        let sig = vector[203, 97, 76, 186, 103, 214, 163, 123, 156, 185, 13, 33, 99, 93, 129, 237, 3, 91, 140, 203, 153,
        240, 190, 254, 5, 73, 91, 129, 145, 17, 17, 155, 23, 236, 240, 192, 203, 75, 204, 120, 29, 227, 135, 32, 111,
        109, 252, 217, 241, 185, 158, 27, 84, 180, 76, 55, 100, 18, 216, 245, 201, 25, 177, 152, 27];

        let hashed_msg = vector[29, 164, 75, 88, 110, 176, 114, 159, 247, 10, 115, 195, 38, 146, 111, 110, 213, 162, 95,
        91, 5, 110, 127, 71, 251, 198, 229, 141, 134, 135, 22, 85];

        let addr1 = vector[76, 191, 102, 143, 202, 111, 16, 208, 31, 22, 17, 34, 83, 64, 68, 67, 107, 128, 112, 46];

        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);

        // Test case from https://goerli.etherscan.io/tx/0x18f72457b356f367db214de9dda07f5d253ebfeb5c426b0d9d5b346b4ba8d021
        // sig: 0x8e809da5ca76e6371ba8dcaa748fc2973f0d9862f76ed08f55b869f5e73591dd24a7367f1ee9e6e3723d13bb0a7092fafb8851f7eecd4a8d34c977013e1551482e
        // hashed_msg: 0x529283629f75203330f0acf68bdbc4e879047fe75da8071c079c495bbb9fb78a
        // addr: 0x4cbf668fca6f10d01f161122534044436b80702e
        let sig = vector[142, 128, 157, 165, 202, 118, 230, 55, 27, 168, 220, 170, 116, 143, 194, 151, 63, 13, 152, 98,
        247, 110, 208, 143, 85, 184, 105, 245, 231, 53, 145, 221, 36, 167, 54, 127, 30, 233, 230, 227, 114, 61, 19, 187,
        10, 112, 146, 250, 251, 136, 81, 247, 238, 205, 74, 141, 52, 201, 119, 1, 62, 21, 81, 72, 46];

        let hashed_msg = vector[82, 146, 131, 98, 159, 117, 32, 51, 48, 240, 172, 246, 139, 219, 196, 232, 121, 4, 127,
        231, 93, 168, 7, 28, 7, 156, 73, 91, 187, 159, 183, 138];

        let addr1 = vector[76, 191, 102, 143, 202, 111, 16, 208, 31, 22, 17, 34, 83, 64, 68, 67, 107, 128, 112, 46];

        let addr = ecrecover_eth_address(sig, hashed_msg);
        assert!(addr == addr1, 0);
    }

    // Helper Move function to recover signature directly to an ETH address.
    fun ecrecover_eth_address(sig: vector<u8>, hashed_msg: vector<u8>): vector<u8> {
        // Normalize the last byte of the signature to be 0 or 1.
        let v = vector::borrow_mut(&mut sig, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };

        let pubkey = ecdsa::ecrecover(&sig, &hashed_msg);
        let uncompressed = ecdsa::decompress_pubkey(&pubkey);

        // Take the last 64 bytes of the uncompressed pubkey.
        let uncompressed_64 = vector::empty<u8>();
        let i = 1;
        while (i < 65) {
            let value = vector::borrow(&uncompressed, i);
            vector::push_back(&mut uncompressed_64, *value);
            i = i + 1;
        };

        // Take the last 20 bytes of the hash of the 64-bytes uncompressed pubkey.
        let hashed = ecdsa::keccak256(&uncompressed_64);
        let addr = vector::empty<u8>();
        let i = 12;
        while (i < 32) {
            let value = vector::borrow(&hashed, i);
            vector::push_back(&mut addr, *value);
            i = i + 1;
        };
        addr
    }

    #[test]
    fun test_preqpare_verifying_key() {
        let vk = x"88c841f7013e91bc61827a64da5f372842e9be522513983253c2a9275e434d93130d100c4b8124fe55dc0dc1ef45918b4d07f0c8b3873b170af258021e71a4dc507aca4fdeafd5dc2f3ee8598117863a57fc25efc408d4227b22e60e8d84bb146e97637d3fbba78a8641f44cfff82cb894472075a6d3515c54ce9fa2ca186f2d5780747b5b7c85e88da7be1a815a3904f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd520afdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b5850200000000000000f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let arr = groth16::pvk_to_bytes(groth16::prepare_verifying_key(&vk));

        let expected_vk_bytes = x"f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let expected_alpha_bytes = x"12168aa38a1ae0360550d0541002b024057ab689d45ce809f8ea36d5286eca9e2f18e70924ac69dcd432228a18036b146aa75a5c17430751f844f686c8ba210c7736adb1851f7afac7fbbc4ac78a01c7ca4508e3d45b5dd31e875c99b0c9d20004f4b3ad8e3c8842b6adc9c3797e3083a31b1ffe654dd4466743cd943b7d3185588a2d81da5f20b36593157c2429b21835964abb93670c81f4a9f230556dcedcc87a5c365613820e225225a650ba7d5a8d283db8317529b37297979ad7576405b26e53f2c162e35557eaf4e59e1b3d456d486291a644fe098f0d29c0435d46e35d114d7357188ed8a8fa26c807fa420e7bff7ce0c2a84a75f189cf6ed039564f36441236720be11bc53850f3700491f50430fe4729676564128f0bf326e67a0038975b396c6fd12c0cd8be75e5985e2841005640b6104b4e1e9817dd3b44e51aa4b0972489ad999bb8143a4e833110057ba32d1ff91c6707b07eab0605b9d6a2745aead54f16a968a4122fa8ca871b70a100b5fd854d4473ec7b519c04547f14b9aba6701e54e737161fc154cc3751f995c0c33d7ef74b893e6bc5514891d73af5543c4ed463e4aebe6cbbd97390bf0bf72075a0649e01a65fa2b7198bedac38406864dc780cb8789df0cb09cf532201d589bc40f84bf6a5816ccbd31ea85d0cf2e06c26037d6970caee38b507450bef282c40366bb4506408f17e331fde3211c0cb021c7858ba83e6a1f1d24bdf550b884d857ff0355ad83cd01346c62dca7197b4d54288ebc982d8228a8403e9a8bd95ef98775bf9c40004e2b5de3e663212";
        let expected_gamma_bytes = x"f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd528a";
        let expected_delta_bytes = x"fdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b505";

        let delta_bytes = vector::pop_back(&mut arr);
        assert!(delta_bytes == expected_delta_bytes, 0);

        let gamma_bytes = vector::pop_back(&mut arr);
        assert!(gamma_bytes == expected_gamma_bytes, 0);

        let alpha_bytes = vector::pop_back(&mut arr);
        assert!(alpha_bytes == expected_alpha_bytes, 0);

        let vk_bytes = vector::pop_back(&mut arr);
        assert!(vk_bytes == expected_vk_bytes, 0);
   }

    #[test]
    #[expected_failure(abort_code = groth16::EInvalidVerifyingKey)]
    fun test_prepare_verifying_key_invalid() {
        let invalid_vk = x"";
        groth16::prepare_verifying_key(&invalid_vk);
    }

    #[test]
    fun test_verify_groth_16_proof() {
        // Success case.
        let vk_bytes = x"f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let alpha_bytes = x"12168aa38a1ae0360550d0541002b024057ab689d45ce809f8ea36d5286eca9e2f18e70924ac69dcd432228a18036b146aa75a5c17430751f844f686c8ba210c7736adb1851f7afac7fbbc4ac78a01c7ca4508e3d45b5dd31e875c99b0c9d20004f4b3ad8e3c8842b6adc9c3797e3083a31b1ffe654dd4466743cd943b7d3185588a2d81da5f20b36593157c2429b21835964abb93670c81f4a9f230556dcedcc87a5c365613820e225225a650ba7d5a8d283db8317529b37297979ad7576405b26e53f2c162e35557eaf4e59e1b3d456d486291a644fe098f0d29c0435d46e35d114d7357188ed8a8fa26c807fa420e7bff7ce0c2a84a75f189cf6ed039564f36441236720be11bc53850f3700491f50430fe4729676564128f0bf326e67a0038975b396c6fd12c0cd8be75e5985e2841005640b6104b4e1e9817dd3b44e51aa4b0972489ad999bb8143a4e833110057ba32d1ff91c6707b07eab0605b9d6a2745aead54f16a968a4122fa8ca871b70a100b5fd854d4473ec7b519c04547f14b9aba6701e54e737161fc154cc3751f995c0c33d7ef74b893e6bc5514891d73af5543c4ed463e4aebe6cbbd97390bf0bf72075a0649e01a65fa2b7198bedac38406864dc780cb8789df0cb09cf532201d589bc40f84bf6a5816ccbd31ea85d0cf2e06c26037d6970caee38b507450bef282c40366bb4506408f17e331fde3211c0cb021c7858ba83e6a1f1d24bdf550b884d857ff0355ad83cd01346c62dca7197b4d54288ebc982d8228a8403e9a8bd95ef98775bf9c40004e2b5de3e663212";
        let gamma_bytes = x"f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd528a";
        let delta_bytes = x"fdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b505";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        let inputs_bytes = x"4af76d91d4bc9a3973c15e3aeb574f0f64547b838f950af35db97f0705c4214b";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"cf4321ae78c61edef79dd0c4b2e6c4a48c24914e9b2b8e6aa9ff0c5e141beae84b80b49510beb90218a76cedb39dcc97fc309ed6d911c8ad65975e081b51c089c95a70ea6dd516ca09c9a59c4ee4f624d645ecbc9fac020194cc0962ab4f040f4d765b0e69014a47bc9f1b06e0ba818bfff2a51f424e3eba325b514e0da88c4e0aae399231bfd8daa29536cf2ddca0986f88147b749d1be59437610aaf7d0c34b200f58e2d2a93f4ecd14208a314583804dd2a3bc283ec00de01ecf789384507";
        let proof = groth16::proof_points_from_bytes(proof_bytes);

        assert!(groth16::verify_groth16_proof(&pvk, &inputs, &proof) == true, 0);

        // Invalid prepared verifying key.
        vector::pop_back(&mut vk_bytes);
        let invalid_pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);
        assert!(groth16::verify_groth16_proof(&invalid_pvk, &inputs, &proof) == false, 0);

        // Invalid public inputs bytes.
        let invalid_inputs = groth16::public_proof_inputs_from_bytes(x"cf");
        assert!(groth16::verify_groth16_proof(&pvk, &invalid_inputs, &proof) == false, 0);

        // Invalid proof bytes.
        let invalid_proof = groth16::proof_points_from_bytes(x"4a");
        assert!(groth16::verify_groth16_proof(&pvk, &inputs, &invalid_proof) == false, 0);
    }
}
