// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::bls12381_tests {
    use sui::bls12381;
    use std::vector;
    use std::hash::sha2_256;
    
    #[test]
    fun test_bls12381_min_sig_valid_sig() {
        let msg = x"0101010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == true, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_sig() {
        let msg = x"0201010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_signature_key_length() {
        let msg = x"0201010101";
        let pk = x"606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e34002e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
    }

    #[test]
    fun test_bls12381_min_sig_invalid_public_key_length() {
        let msg = x"0201010101";
        let pk = x"606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let verify = bls12381::bls12381_min_sig_verify(&sig, &pk, &msg);
        assert!(verify == false, 0)
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
}