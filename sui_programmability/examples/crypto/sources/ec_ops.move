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
    use sui::ristretto255;
    use std::hash::sha2_256;
    #[test_only]
    use sui::test_utils::assert_eq;

    const EInvalidLength: u64 = 0;

    const BLS12381_ORDER: vector<u8> = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";

    ////////////////////////////////////////
    ////// BLS signature verification //////

    fun bls_min_sig_verify(msg: &vector<u8>, pk: &Element<bls12381::G2>, sig: &Element<bls12381::G1>): bool {
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

    // An encryption of m under pk is (r*G, r*pk + m) for random r.
    struct ElGamalEncryption has drop, store {
        ephemeral: Element<ristretto255::G>,
        ciphertext: Element<ristretto255::G>,
    }

    #[test_only]
    fun insecure_elgamal_key_gen(sk: u64): (Element<ristretto255::Scalar>, Element<ristretto255::G>) {
        let sk = ristretto255::scalar_from_u64(sk);
        let pk = ristretto255::g_mul(&sk, &ristretto255::g_generator());
        (sk, pk)
    }

    #[test_only]
    fun insecure_elgamal_encrypt(
        pk: &Element<ristretto255::G>,
        r: u64,
        m: &Element<ristretto255::G>
    ): ElGamalEncryption {
        let r = ristretto255::scalar_from_u64(r);
        let ephemeral = ristretto255::g_mul(&r, &ristretto255::g_generator());
        let pk_r  = ristretto255::g_mul(&r, pk);
        let ciphertext = ristretto255::g_add(m, &pk_r);
        ElGamalEncryption { ephemeral, ciphertext }
    }

    fun elgamal_decrypt(sk: &Element<ristretto255::Scalar>, enc: &ElGamalEncryption): Element<ristretto255::G> {
        let pk_r = ristretto255::g_mul(sk, &enc.ephemeral);
        ristretto255::g_sub(&enc.ciphertext, &pk_r)
    }

    // Basic sigma protocol for proving equality of two ElGamal encryptions.
    // See https://crypto.stackexchange.com/questions/30010/is-there-a-way-to-prove-equality-of-plaintext-that-was-encrypted-using-different
    struct EqualityProof has drop, store {
        a1: Element<ristretto255::G>,
        a2: Element<ristretto255::G>,
        a3: Element<ristretto255::G>,
        z1: Element<ristretto255::Scalar>,
        z2: Element<ristretto255::Scalar>,
    }

    fun fiat_shamir_challenge(
        pk1: &Element<ristretto255::G>,
        pk2: &Element<ristretto255::G>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        a1: &Element<ristretto255::G>,
        a2: &Element<ristretto255::G>,
        a3: &Element<ristretto255::G>,
    ): Element<ristretto255::Scalar> {
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
        ristretto255::scalar_from_bytes(&hash)
    }

    #[test_only]
    fun insecure_equility_prove(
        pk1: &Element<ristretto255::G>,
        pk2: &Element<ristretto255::G>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        sk1: &Element<ristretto255::Scalar>,
        r2:  u64,
        r: u64,
    ): EqualityProof {
        let b1 = ristretto255::scalar_from_u64(r);
        let b2 = ristretto255::scalar_from_u64(r+123);
        let r2 = ristretto255::scalar_from_u64(r2);

        // a1 = b1*G (for proving knowledge of sk1)
        let a1 = ristretto255::g_mul(&b1, &ristretto255::g_generator());
        // a2 = b2*g (for proving knowledge of r2)
        let a2 = ristretto255::g_mul(&b2, &ristretto255::g_generator());
        let scalars = vector::singleton(b1);
        vector::push_back(&mut scalars, ristretto255::scalar_neg(&b2));
        let points = vector::singleton(enc1.ephemeral);
        vector::push_back(&mut points, *pk2);
        let a3 = ristretto255::g_multi_scalar_multiplication(&scalars, &points);
        // RO challenge
        let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &a1, &a2, &a3);
        // z1 = b1 + c*sk1
        let z1 = ristretto255::scalar_add(&ristretto255::scalar_mul(&c, sk1), &b1);
        // z2 = b2 + c*r2
        let z2 = ristretto255::scalar_add(&ristretto255::scalar_mul(&c, &r2), &b2);

        EqualityProof { a1, a2, a3, z1, z2 }
    }

    fun equility_verify(
        pk1: &Element<ristretto255::G>,
        pk2: &Element<ristretto255::G>,
        enc1: &ElGamalEncryption,
        enc2: &ElGamalEncryption,
        proof: &EqualityProof
    ): bool {
        let c = fiat_shamir_challenge(pk1, pk2, enc1, enc2, &proof.a1, &proof.a2, &proof.a3);
        // Check if z1*G = a1 + c*pk1
        let lhs = ristretto255::g_mul(&proof.z1, &ristretto255::g_generator());
        let pk1_c = ristretto255::g_mul(&c, pk1);
        let rhs = ristretto255::g_add(&proof.a1, &pk1_c);
        if (!group_ops::equal(&lhs, &rhs)) {
            return false
        };
        // Check if z2*G = a2 + c*eph2
        let lhs = ristretto255::g_mul(&proof.z2, &ristretto255::g_generator());
        let eph2_c = ristretto255::g_mul(&c, &enc2.ephemeral);
        let rhs = ristretto255::g_add(&proof.a2, &eph2_c);
        if (!group_ops::equal(&lhs, &rhs)) {
            return false
        };
        // Check if a3 = c*(ct2 - ct1) + z1*eph1 - z2*pk2
        let scalars = vector::singleton(c);
        vector::push_back(&mut scalars, ristretto255::scalar_neg(&c));
        vector::push_back(&mut scalars, proof.z1);
        vector::push_back(&mut scalars, ristretto255::scalar_neg(&proof.z2));
        let points = vector::singleton(enc2.ciphertext);
        vector::push_back(&mut points, enc1.ciphertext);
        vector::push_back(&mut points, enc1.ephemeral);
        vector::push_back(&mut points, *pk2);
        let lhs = ristretto255::g_multi_scalar_multiplication(&scalars, &points);
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
        // Now, a sender wishes to send an encrypted message to pk1.
        let m = ristretto255::g_mul(&ristretto255::scalar_from_u64(5555), &ristretto255::g_generator());
        let enc1 = insecure_elgamal_encrypt(&pk1, 1234, &m);
        // The first party decrypts the message.
        let m1 = elgamal_decrypt(&sk1, &enc1);
        assert_eq(m, m1);
        // Now, the first party wishes to send an encrypted message to pk2.
        let r2 = 4321;
        let enc2 = insecure_elgamal_encrypt(&pk2, r2, &m);
        // And to prove equality of the two encrypted messages.
        let proof = insecure_equility_prove(&pk1, &pk2, &enc1, &enc2, &sk1, r2, 8888);
        // Anyone can verify it.
        assert!(equility_verify(&pk1, &pk2, &enc1, &enc2, &proof), 0);

        // Proving with an invalid witness should result in a failed verification.
        let bad_r2 = 1111;
        let proof = insecure_equility_prove(&pk1, &pk2, &enc1, &enc2, &sk1, bad_r2, 8888);
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
    fun ibe_decrypt(enc: IbeEncryption, target_key: &Element<bls12381::G1>): Option<vector<u8>> {
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


    // Broken since the current chain does not follow the RFC - https://github.com/drand/kyber-bls12381/issues/22
    // TODO: Update once the new chain is deployed.
    #[test]
    fun test_ibe_decrypt_drand() {
        // Retrieved using 'curl https://api.drand.sh/dbd506d6ef76e5f386f41c651dcb808c5bcbd75471cc4eafa3f4df7ad4e4c493/info'
        let round = 2594767;
        let pk = x"a0b862a7527fee3a731bcb59280ab6abd62d5c0b6ea03dc4ddf6612fdfc9d01f01c31542541771903475eb1ec6615f8d0df0b8b6dce385811d6dcf8cbefb8759e5e616a3dfd054c928940766d9a5b9db91e3b697e5d70a975181e007f87fca5e";
        let pk = bls12381::g2_from_bytes(&pk);
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

        // Retreived using 'curl https://api.drand.sh/dbd506d6ef76e5f386f41c651dcb808c5bcbd75471cc4eafa3f4df7ad4e4c493/public/2594767'.
        let sig = x"a8deec780e592680581d1d96ca5a4c743a37b1b961684cd357b80f7401be5cfb38b9f5ed1dbb6b49684caff0360453f2";
        let target_key = bls12381::g1_from_bytes(&sig);
        // assert!(bls_min_sig_verify(&target, &pk, &sig), 0);

        let enc = insecure_ibe_encrypt(&pk, &target, &msg, &x"1234567890");
        let decrypted_msg = ibe_decrypt(enc, &target_key);
        assert!(option::extract(&mut decrypted_msg) == msg, 0);
    }

    /////////////////////////////////////////////////////////////////////////////////
    ////// Helper functions for converting 32 byte vectors to BLS12-381 order  //////

    // Returns x-ORDER if x >= ORDER, otherwise none.
    fun try_substract(x: &vector<u8>): Option<vector<u8>> {
        assert!(vector::length(x) == 32, EInvalidLength);
        let c = vector::empty();
        let i = 0;
        let carry: u8 = 0;
        while (i < 32) {
            let curr = 31 - i;
            let b1 = *vector::borrow(x, curr);
            let b2 = *vector::borrow(&BLS12381_ORDER, curr);
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

    // TODO: KZG commitment verification
    // TODO: Groth16 proof verification
}