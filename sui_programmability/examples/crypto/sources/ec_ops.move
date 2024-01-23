// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Examples of cryptographic primitives that can be implemented in Move using group opeartions.
//
// Functions with the prefix "insecure" are here for testing, but should be called off-chain (probably implemented in
// other languages) to avoid leaking secrets.
module crypto::ec_ops {

    use sui::bls12381;
    use sui::group_ops::Element;
    use sui::group_ops;
    use std::vector;
    use sui::hash::blake2b256;
    use std::option::Option;
    use std::option;
    #[test_only]
    use std::hash::sha2_256;
    #[test_only]
    use sui::test_utils::assert_eq;

    const EInvalidLength: u64 = 0;

    const BLS12381_ORDER: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";

    ////////////////////////////////////////
    ////// BLS signature verification //////

    public fun bls_min_sig_verify(msg: &vector<u8>, pk: &Element<bls12381::G2>, sig: &Element<bls12381::G1>): bool {
        let hashed_msg = bls12381::hash_to_g1(msg);
        let lhs = bls12381::pairing(&hashed_msg, pk);
        let rhs = bls12381::pairing(sig, &bls12381::g2_generator());
        group_ops::equal(&lhs, &rhs)
    }

    #[test]
    fun test_bls_min_sig_verify() {
        let msg = x"0101010101";
        let pk = x"8df101606f91f3cad7f54b8aff0f0f64c41c482d9b9f9fe81d2b607bc5f611bdfa8017cf04b47b44b222c356ef555fbd11058c52c077f5a7ec6a15ccfd639fdc9bd47d005a111dd6cdb8c02fe49608df55a3c9822986ad0b86bdea3abfdfe464";
        let sig = x"908e345f2e2803cd941ae88c218c96194233c9053fa1bca52124787d3cca141c36429d7652435a820c72992d5eee6317";

        let pk = bls12381::g2_from_bytes(&pk);
        let sig= bls12381::g1_from_bytes(&sig);
        assert!(bls_min_sig_verify(&msg, &pk, &sig), 0);
    }


    ////////////////////////////////////////////////////////////////
    ////// Proof of plaintext equality of ElGamal encryptions //////

    // An encryption of group element m under pk is (r*G, r*pk + m) for random r.
    struct ElGamalEncryption has drop, store {
        ephemeral: Element<bls12381::G1>,
        ciphertext: Element<bls12381::G1>,
    }

    // The following is insecure since the secret key is small, but in practice it should be a random scalar.
    #[test_only]
    fun insecure_elgamal_key_gen(sk: u64): (Element<bls12381::Scalar>, Element<bls12381::G1>) {
        let sk = bls12381::scalar_from_u64(sk);
        let pk = bls12381::g1_mul(&sk, &bls12381::g1_generator());
        (sk, pk)
    }

    // The following is insecure since the nonce is small, but in practice it should be a random scalar.
    #[test_only]
    fun insecure_elgamal_encrypt(
        pk: &Element<bls12381::G1>,
        r: u64,
        m: &Element<bls12381::G1>
    ): ElGamalEncryption {
        let r = bls12381::scalar_from_u64(r);
        let ephemeral = bls12381::g1_mul(&r, &bls12381::g1_generator());
        let pk_r  = bls12381::g1_mul(&r, pk);
        let ciphertext = bls12381::g1_add(m, &pk_r);
        ElGamalEncryption { ephemeral, ciphertext }
    }

    public fun elgamal_decrypt(sk: &Element<bls12381::Scalar>, enc: &ElGamalEncryption): Element<bls12381::G1> {
        let pk_r = bls12381::g1_mul(sk, &enc.ephemeral);
        bls12381::g1_sub(&enc.ciphertext, &pk_r)
    }

    // Basic sigma protocol for proving equality of two ElGamal encryptions.
    // See https://crypto.stackexchange.com/questions/30010/is-there-a-way-to-prove-equality-of-plaintext-that-was-encrypted-using-different
    struct EqualityProof has drop, store {
        a1: Element<bls12381::G1>,
        a2: Element<bls12381::G1>,
        a3: Element<bls12381::G1>,
        z1: Element<bls12381::Scalar>,
        z2: Element<bls12381::Scalar>,
    }

    public fun fiat_shamir_challenge(
        pk1: &Element<bls12381::G1>,
        pk2: &Element<bls12381::G1>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        a1: &Element<bls12381::G1>,
        a2: &Element<bls12381::G1>,
        a3: &Element<bls12381::G1>,
    ): Element<bls12381::Scalar> {
        let to_hash = vector::empty<u8>();
        vector::append(&mut to_hash, *group_ops::bytes(pk1));
        vector::append(&mut to_hash, *group_ops::bytes(pk2));
        vector::append(&mut to_hash, *group_ops::bytes(&enc1.ephemeral));
        vector::append(&mut to_hash, *group_ops::bytes(&enc1.ciphertext));
        vector::append(&mut to_hash, *group_ops::bytes(&enc2.ephemeral));
        vector::append(&mut to_hash, *group_ops::bytes(&enc2.ciphertext));
        vector::append(&mut to_hash, *group_ops::bytes(a1));
        vector::append(&mut to_hash, *group_ops::bytes(a2));
        vector::append(&mut to_hash, *group_ops::bytes(a3));
        let hash = blake2b256(&to_hash);
        // Make sure we are in the right field. Note that for security we only need the lower 128 bits.
        let len = vector::length(&hash);
        *vector::borrow_mut(&mut hash, len-1) = 0;
        bls12381::scalar_from_bytes(&hash)
    }

    // The following is insecure since the nonces are small, but in practice they should be random scalars.
    #[test_only]
    fun insecure_equility_prove(
        pk1: &Element<bls12381::G1>,
        pk2: &Element<bls12381::G1>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        sk1: &Element<bls12381::Scalar>,
        r1: u64,
        r2: u64,
    ): EqualityProof {
        let b1 = bls12381::scalar_from_u64(r1);
        let b2 = bls12381::scalar_from_u64(r1+123);
        let r2 = bls12381::scalar_from_u64(r2);

        // a1 = b1*G (for proving knowledge of sk1)
        let a1 = bls12381::g1_mul(&b1, &bls12381::g1_generator());
        // a2 = b2*g (for proving knowledge of r2)
        let a2 = bls12381::g1_mul(&b2, &bls12381::g1_generator());
        let scalars = vector::singleton(b1);
        vector::push_back(&mut scalars, bls12381::scalar_neg(&b2));
        let points = vector::singleton(enc1.ephemeral);
        vector::push_back(&mut points, *pk2);
        let a3 = bls12381::g1_multi_scalar_multiplication(&scalars, &points);
        // RO challenge
        let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &a1, &a2, &a3);
        // z1 = b1 + c*sk1
        let z1 = bls12381::scalar_add(&bls12381::scalar_mul(&c, sk1), &b1);
        // z2 = b2 + c*r2
        let z2 = bls12381::scalar_add(&bls12381::scalar_mul(&c, &r2), &b2);

        EqualityProof { a1, a2, a3, z1, z2 }
    }

    public fun equility_verify(
        pk1: &Element<bls12381::G1>,
        pk2: &Element<bls12381::G1>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        proof: &EqualityProof
    ): bool {
        let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &proof.a1, &proof.a2, &proof.a3);
        // Check if z1*G = a1 + c*pk1
        let lhs = bls12381::g1_mul(&proof.z1, &bls12381::g1_generator());
        let pk1_c = bls12381::g1_mul(&c, pk1);
        let rhs = bls12381::g1_add(&proof.a1, &pk1_c);
        if (!group_ops::equal(&lhs, &rhs)) {
            return false
        };
        // Check if z2*G = a2 + c*eph2
        let lhs = bls12381::g1_mul(&proof.z2, &bls12381::g1_generator());
        let eph2_c = bls12381::g1_mul(&c, &enc2.ephemeral);
        let rhs = bls12381::g1_add(&proof.a2, &eph2_c);
        if (!group_ops::equal(&lhs, &rhs)) {
            return false
        };
        // Check if a3 = c*(ct2 - ct1) + z1*eph1 - z2*pk2
        let scalars = vector::singleton(c);
        vector::push_back(&mut scalars, bls12381::scalar_neg(&c));
        vector::push_back(&mut scalars, proof.z1);
        vector::push_back(&mut scalars, bls12381::scalar_neg(&proof.z2));
        let points = vector::singleton(enc2.ciphertext);
        vector::push_back(&mut points, enc1.ciphertext);
        vector::push_back(&mut points, enc1.ephemeral);
        vector::push_back(&mut points, *pk2);
        let lhs = bls12381::g1_multi_scalar_multiplication(&scalars, &points);
        if (!group_ops::equal(&lhs, &proof.a3)) {
            return false
        };

        return true
    }

    #[test]
    fun test_elgamal_ops() {
        // We have two parties.
        let (sk1, pk1) = insecure_elgamal_key_gen(2110);
        let (_, pk2) = insecure_elgamal_key_gen(1021);
        // A sender wishes to send an encrypted message to pk1.
        let m = bls12381::g1_mul(&bls12381::scalar_from_u64(5555), &bls12381::g1_generator());
        let enc1 = insecure_elgamal_encrypt(&pk1, 1234, &m);
        // The first party decrypts the message.
        let m1 = elgamal_decrypt(&sk1, &enc1);
        assert_eq(m, m1);
        // Now, the first party wishes to send an encrypted message to pk2.
        let r2 = 4321;
        let enc2 = insecure_elgamal_encrypt(&pk2, r2, &m);
        // And to prove equality of the two encrypted messages.
        let proof = insecure_equility_prove(&pk1, &pk2, &enc1, &enc2, &sk1,  8888, r2);
        // Anyone can verify it.
        assert!(equility_verify(&pk1, &pk2, &enc1, &enc2, &proof), 0);

        // Proving with an invalid witness should result in a failed verification.
        let bad_r2 = 1111;
        let proof = insecure_equility_prove(&pk1, &pk2, &enc1, &enc2, &sk1, 8888, bad_r2);
        assert!(!equility_verify(&pk1, &pk2, &enc1, &enc2, &proof), 0);
    }

    ////////////////////////////
    ////// IBE decryption //////

    struct IbeEncryption has store, drop, copy {
        u: Element<bls12381::G2>,
        v: vector<u8>,
        w: vector<u8>,
    }

    // Encrypt a message 'm' for 'target'. Follows the algorithms of https://eprint.iacr.org/2023/189.pdf.
    #[test_only]
    fun insecure_ibe_encrypt(pk: &Element<bls12381::G2>, target: &vector<u8>, m: &vector<u8>, sigma: &vector<u8>): IbeEncryption {
        assert!(vector::length(target) <= 32, 0);
        // r = H(sigma | m) as a scalar
        let to_hash = vector::empty<u8>();
        vector::append(&mut to_hash, *sigma);
        vector::append(&mut to_hash, *m);
        let r = modulo_order(&blake2b256(&to_hash));
        let r = bls12381::scalar_from_bytes(&r);
        // U = r*g2
        let u = bls12381::g2_mul(&r, &bls12381::g2_generator());
        // V = sigma xor H(e(H(target), pk^r))
        let pk_r = bls12381::g2_mul(&r, pk);
        let target_hash = bls12381::hash_to_g1(target);
        let e = bls12381::pairing(&target_hash, &pk_r);
        let hash = blake2b256(group_ops::bytes(&e));
        let v = vector::empty();
        let i = 0;
        while (i < vector::length(sigma)) {
            vector::push_back(&mut v, *vector::borrow(sigma, i) ^ *vector::borrow(&hash, i));
            i = i + 1;
        };
        // W = m xor H(sigma)
        let hash = blake2b256(sigma);
        let w = vector::empty();
        let i = 0;
        while (i < vector::length(m)) {
            vector::push_back(&mut w, *vector::borrow(m, i) ^ *vector::borrow(&hash, i));
            i = i + 1;
        };
        IbeEncryption { u, v, w }
    }

    // Decrypt an IBE encryption using a 'target_key'.
    public fun ibe_decrypt(enc: IbeEncryption, target_key: &Element<bls12381::G1>): Option<vector<u8>> {
        // sigma_prime = V xor H(e(H(target), pk^r))
        let e = bls12381::pairing(target_key, &enc.u);
        let hash = blake2b256(group_ops::bytes(&e));
        let sigma_prime = vector::empty();
        let i = 0;
        while (i < vector::length(&enc.v)) {
            vector::push_back(&mut sigma_prime, *vector::borrow(&hash, i) ^ *vector::borrow(&enc.v, i));
            i = i + 1;
        };
        // m_prime = W xor H(sigma_prime)
        let hash = blake2b256(&sigma_prime);
        let m_prime = vector::empty();
        let i = 0;
        while (i < vector::length(&enc.w)) {
            vector::push_back(&mut m_prime, *vector::borrow(&hash, i) ^ *vector::borrow(&enc.w, i));
            i = i + 1;
        };
        // r = H(sigma_prime | m_prime) as a scalar
        let to_hash = vector::empty<u8>();
        vector::append(&mut to_hash, sigma_prime);
        vector::append(&mut to_hash, m_prime);
        let r = modulo_order(&blake2b256(&to_hash));
        let r = bls12381::scalar_from_bytes(&r);
        // U ?= r*g2
        let g2r = bls12381::g2_mul(&r, &bls12381::g2_generator());
        if (group_ops::equal(&enc.u, &g2r)) {
            option::some(m_prime)
        } else {
            option::none()
        }
    }

    // This test emulates drand based timelock encryption.
    #[test]
    fun test_ibe_decrypt_drand() {
        // Retrieved using 'curl https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/info'
        let round = 1234;
        let pk_bytes = x"83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a";
        let pk = bls12381::g2_from_bytes(&pk_bytes);
        let msg = x"0101010101";

        // Derive the 'target' for the specific round (see drand_lib.move).
        let round_bytes: vector<u8> = vector[0, 0, 0, 0, 0, 0, 0, 0];
        let i = 7;
        while (i > 0) {
            let curr_byte = round % 0x100;
            let curr_element = vector::borrow_mut(&mut round_bytes, i);
            *curr_element = (curr_byte as u8);
            round = round >> 8;
            i = i - 1;
        };
        let target = sha2_256(round_bytes);

        // Retreived with 'curl https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971/public/1234'.
        let sig_bytes = x"a81d4aad15461a0a02b43da857be1d782a2232a3c7bb370a2763e95ce1f2628460b24de2cee7453cd12e43c197ea2f23";
        let target_key = bls12381::g1_from_bytes(&sig_bytes);
        assert!(bls12381::bls12381_min_sig_verify(&sig_bytes, &pk_bytes, &target), 0);

        let enc = insecure_ibe_encrypt(&pk, &target, &msg, &x"1234567890");
        let decrypted_msg = ibe_decrypt(enc, &target_key);
        assert!(option::extract(&mut decrypted_msg) == msg, 0);
    }

    ///////////////////////////////////////////////////////////////////////////////////
    ////// Helper functions for converting 32 byte vectors to BLS12-381's order  //////

    // Returns x-ORDER if x >= ORDER, otherwise none.
    fun try_substract(x: &vector<u8>): Option<vector<u8>> {
        assert!(vector::length(x) == 32, EInvalidLength);
        let order = BLS12381_ORDER;
        let c = vector::empty();
        let i = 0;
        let carry: u8 = 0;
        while (i < 32) {
            let curr = 31 - i;
            let b1 = *vector::borrow(x, curr);
            let b2 = *vector::borrow(&order, curr);
            let sum: u16 = (b2 as u16) + (carry as u16);
            if (sum > (b1 as u16)) {
                carry = 1;
                let res = 0x100 + (b1 as u16) - sum;
                vector::push_back(&mut c, (res as u8));
            } else {
                carry = 0;
                let res = (b1 as u16) - sum;
                vector::push_back(&mut c, (res as u8));
            };
            i = i + 1;
        };
        if (carry != 0) {
            option::none()
        } else {
            vector::reverse(&mut c);
            option::some(c)
        }
    }

    fun modulo_order(x: &vector<u8>): vector<u8> {
        let res = *x;
        // Since 2^256 < 3*ORDER, this loop won't run many times.
        while (true) {
            let minus_order = try_substract(&res);
            if (option::is_none(&minus_order)) {
                return res
            };
            res = *option::borrow(&minus_order);
        };
        res
    }

    #[test]
    fun test_try_substract_and_modulo() {
        let smaller: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000";
        let res = try_substract(&smaller);
        assert!(option::is_none(&res), 0);

        let bigger: vector<u8> = x"8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000fffffff5";
        let res = try_substract(&bigger);
        assert!(option::is_some(&res), 0);
        let bigger_minus_order = *option::borrow(&res);
        let expected: vector<u8> = x"1824b159acc5056f998c4fefecbc4ff55884b7fa0003480200000001fffffff4";
        assert_eq(bigger_minus_order, expected);

        let larger: vector<u8> = x"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff6";
        let expected: vector<u8> = x"1824b159acc5056f998c4fefecbc4ff55884b7fa0003480200000001fffffff4";
        let modulo = modulo_order(&larger);
        assert!(modulo == expected, 0);
    }

    // TODO: Groth16 proof verification
}
