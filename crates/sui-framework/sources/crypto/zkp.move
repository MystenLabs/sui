// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::zkp {
    use std::vector;

    /// Length of the vector<u8> representing a SHA3-256 digest.
    const LENGTH: u64 = 32;

    /// Error code when the length is invalid.
    const LengthMismatch: u64 = 0;

    /// Proof<Bls12_381>
    struct Proof has store, copy, drop {
        bytes: vector<u8>
    }

    /// PreparedVerifyingKey
    struct PreparedVerifyingKey has store, copy, drop {
        /// The element vk.gamma_abc_g1,
        /// aka the `[gamma^{-1} * (beta * a_i + alpha * b_i + c_i) * G]`, where i spans the public inputs
        vk_gamma_abc_g1: vector<u8>,
        /// The element `e(alpha * G, beta * H)` in `E::GT`. blst_fp12
        alpha_g1_beta_g2: vector<u8>,
        /// The element `- gamma * H` in `E::G2`, for use in pairings.
        gamma_g2_neg_pc: vector<u8>,
        /// The element `- delta * H` in `E::G2`, for use in pairings.
        delta_g2_neg_pc: vector<u8>,
    }

    public fun pvk_from_bytes(
        vk_gamma_abc_g1: vector<u8>, 
        alpha_g1_beta_g2: vector<u8>, 
        gamma_g2_neg_pc: vector<u8>, 
        delta_g2_neg_pc: vector<u8>): PreparedVerifyingKey {
        PreparedVerifyingKey { vk_gamma_abc_g1, alpha_g1_beta_g2, gamma_g2_neg_pc, delta_g2_neg_pc }
    }
    
    public fun proof_from_bytes(bytes: vector<u8>): Proof {
        Proof { bytes }
    }
    /// @param pvk: PreparedVerifyingKey
    ///
    /// @param x
    ///
    /// @param proof
    /// Returns the validity of the Groth16 proof passed as argument.
    public fun verify_groth16_proof(pvk: PreparedVerifyingKey, x: vector<u8>, proof: Proof): bool {
        internal_verify_groth16_proof(
            pvk.vk_gamma_abc_g1, 
            pvk.alpha_g1_beta_g2, 
            pvk.gamma_g2_neg_pc, 
            pvk.delta_g2_neg_pc, 
            x, 
            proof.bytes
        )
    }

    public native fun internal_verify_groth16_proof(
        vk_gamma_abc_g1_bytes: vector<u8>, 
        alpha_g1_beta_g2_bytes: vector<u8>, 
        gamma_g2_neg_pc_bytes: vector<u8>, 
        delta_g2_neg_pc_bytes: vector<u8>, 
        x_bytes: vector<u8>, 
        proof_bytes: vector<u8>
    ): bool;
}
