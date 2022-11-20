module games::drand_lib {
    use std::vector;

    use sui::bls12381;
    use std::hash::sha2_256;

    friend games::drand_based_lottery;

    /// Error codes
    const EInvalidRndLength: u64 = 0;

    public(friend) fun verify_drand_signature(sig: vector<u8>, prev_sig: vector<u8>, round: u64): bool {
        // The public key of chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce.
        let drand_public_key: vector<u8> =
            x"868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31";
        // Convert round to a byte array in big-endian order.
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };

        // Compute sha256(prev_sig, round_bytes).
        vector::append(&mut prev_sig, round_bytes);
        let digest = sha2_256(prev_sig);

        // Verify the signature on the hash.
        bls12381::bls12381_min_pk_verify(&sig, &drand_public_key, &digest)
    }

    public(friend) fun derive_randomness(drand_sig: vector<u8>): vector<u8> {
        sha2_256(drand_sig)
    }

    // Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
    // Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.
    public(friend) fun safe_selection(n: u64, rnd: vector<u8>): u64 {
        assert!(vector::length(&rnd) >= 16, EInvalidRndLength);
        let m: u128 = 0;
        let i = 0;
        while (i < 16) {
            m = m << 8;
            let curr_byte = *vector::borrow(&rnd, i);
            m = m + (curr_byte as u128);
            i = i + 1;
        };
        let n_128 = (n as u128);
        let module_128  = m % n_128;
        let res = (module_128 as u64);
        res
    }
}