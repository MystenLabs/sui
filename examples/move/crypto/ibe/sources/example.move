// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of using tlock or IBE decryption in Move.
module ibe::example;

use sui::{bls12381::{Self, G1, G2}, group_ops::{bytes, equal, Element}, hash::blake2b256};

const EInvalidLength: u64 = 1;

/// An encryption of 32 bytes message following https://eprint.iacr.org/2023/189.pdf.
public struct IbeEncryption has store, drop, copy {
    u: Element<G2>,
    v: vector<u8>,
    w: vector<u8>,
}

public fun from_bytes(bytes: vector<u8>): IbeEncryption {
    let mut buffer = vector::empty();
    let mut i = 0;
    while (i < 96) {
        buffer.push_back(bytes[i]);
        i = i + 1;
    };
    let u = bls12381::g2_from_bytes(&buffer);

    let mut v = vector::empty();
    while (i < 96 + 32) {
        v.push_back(bytes[i]);
        i = i + 1;
    };

    let mut w = vector::empty();
    while (i < 96 + 32 + 32) {
        w.push_back(bytes[i]);
        i = i + 1;
    };

    IbeEncryption { u, v, w }
}

#[test_only]
/// Encrypt a message 'm' for 'target'. Follows the algorithms of https://eprint.iacr.org/2023/189.pdf.
/// Note that the algorithms in that paper use G2 for signatures, where the drand chain uses G1, thus
/// the operations below are slightly different.
public fun insecure_ibe_encrypt(
    pk: &Element<G2>,
    target: &vector<u8>,
    m: &vector<u8>,
    sigma: &vector<u8>,
): IbeEncryption {
    assert!(sigma.length() == 32, 0);
    // pk_rho = e(H1(target), pk)
    let target_hash = bls12381::hash_to_g1(target);
    let pk_rho = bls12381::pairing(&target_hash, pk);

    // r = H3(sigma | m) as a scalar
    assert!(m.length() == sigma.length(), 0);
    let mut to_hash = b"HASH3 - ";
    to_hash.append(*sigma);
    to_hash.append(*m);
    let r = modulo_order(&blake2b256(&to_hash));
    let r = bls12381::scalar_from_bytes(&r);

    // U = r*g2
    let u = bls12381::g2_mul(&r, &bls12381::g2_generator());

    // V = sigma xor H2(pk_rho^r)
    let pk_rho_r = bls12381::gt_mul(&r, &pk_rho);
    let mut to_hash = b"HASH2 - ";
    to_hash.append(*bytes(&pk_rho_r));
    let hash_pk_rho_r = blake2b256(&to_hash);
    let mut v = vector::empty();
    let mut i = 0;
    while (i < sigma.length()) {
        v.push_back(sigma[i] ^ hash_pk_rho_r[i]);
        i = i + 1;
    };

    // W = m xor H4(sigma)
    let mut to_hash = b"HASH4 - ";
    to_hash.append(*sigma);
    let hash = blake2b256(&to_hash);
    let mut w = vector::empty();
    let mut i = 0;
    while (i < m.length()) {
        w.push_back(m[i] ^ hash[i]);
        i = i + 1;
    };

    IbeEncryption { u, v, w }
}

/// Decrypt an IBE encryption using a 'target_key'.
public fun ibe_decrypt(enc: IbeEncryption, target_key: &Element<G1>): Option<vector<u8>> {
    // sigma_prime = V xor H2(e(target_key, u))
    let e = bls12381::pairing(target_key, &enc.u);
    let mut to_hash = b"HASH2 - ";
    to_hash.append(*bytes(&e));
    let hash = blake2b256(&to_hash);
    let mut sigma_prime = vector::empty();
    let mut i = 0;
    while (i < enc.v.length()) {
        sigma_prime.push_back(hash[i] ^ enc.v[i]);
        i = i + 1;
    };

    // m_prime = W xor H4(sigma_prime)
    let mut to_hash = b"HASH4 - ";
    to_hash.append(sigma_prime);
    let hash = blake2b256(&to_hash);
    let mut m_prime = vector::empty();
    let mut i = 0;
    while (i < enc.w.length()) {
        m_prime.push_back(hash[i] ^ enc.w[i]);
        i = i + 1;
    };

    // r = H3(sigma_prime | m_prime) as a scalar (the paper has a typo)
    let mut to_hash = b"HASH3 - ";
    to_hash.append(sigma_prime);
    to_hash.append(m_prime);
    // If the encryption is generated correctly, this should always be a valid scalar (before the modulo).
    // However since in the tests we create it insecurely, we make sure it is in the right range.
    let r = modulo_order(&blake2b256(&to_hash));
    let r = bls12381::scalar_from_bytes(&r);

    // U ?= r*g2
    let g2r = bls12381::g2_mul(&r, &bls12381::g2_generator());
    if (equal(&enc.u, &g2r)) {
        option::some(m_prime)
    } else {
        option::none()
    }
}

///////////////////////////////////////////////////////////////////////////////////
////// Helper functions for converting 32 byte vectors to BLS12-381's order  //////

/// Returns x-ORDER if x >= ORDER, otherwise none.
public(package) fun try_substract(x: &vector<u8>): Option<vector<u8>> {
    assert!(x.length() == 32, EInvalidLength);
    let order = x"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";
    let mut c = vector::empty();
    let mut i = 0;
    let mut carry: u8 = 0;
    while (i < 32) {
        let curr = 31 - i;
        let b1 = x[curr];
        let b2 = order[curr];
        let sum: u16 = (b2 as u16) + (carry as u16);
        if (sum > (b1 as u16)) {
            carry = 1;
            let res = 0x100 + (b1 as u16) - sum;
            c.push_back(res as u8);
        } else {
            carry = 0;
            let res = (b1 as u16) - sum;
            c.push_back(res as u8);
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

public(package) fun modulo_order(x: &vector<u8>): vector<u8> {
    let mut res = *x;
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
