// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::groth16 {
    use std::vector;

    // Error for input is not a valid Arkwork representation of a verifying key.
    const EInvalidVerifyingKey: u64 = 0;

    /// A `PreparedVerifyingKey` consisting of four components in serialized form.
    struct PreparedVerifyingKey has store, copy, drop {
        vk_gamma_abc_g1_bytes: vector<u8>,
        alpha_g1_beta_g2_bytes: vector<u8>,
        gamma_g2_neg_pc_bytes: vector<u8>,
        delta_g2_neg_pc_bytes: vector<u8>
    }

    /// Creates a `PreparedVerifyingKey` from bytes.
    public fun pvk_from_bytes(vk_gamma_abc_g1_bytes: vector<u8>, alpha_g1_beta_g2_bytes: vector<u8>, gamma_g2_neg_pc_bytes: vector<u8>, delta_g2_neg_pc_bytes: vector<u8>): PreparedVerifyingKey {
        PreparedVerifyingKey {
            vk_gamma_abc_g1_bytes,
            alpha_g1_beta_g2_bytes,
            gamma_g2_neg_pc_bytes,
            delta_g2_neg_pc_bytes
        }
    }

    /// Returns bytes of the four components of the `PreparedVerifyingKey`.
    public fun pvk_to_bytes(pvk: PreparedVerifyingKey): vector<vector<u8>> {
        let res = vector::empty();
        vector::push_back(&mut res, pvk.vk_gamma_abc_g1_bytes);
        vector::push_back(&mut res, pvk.alpha_g1_beta_g2_bytes);
        vector::push_back(&mut res, pvk.gamma_g2_neg_pc_bytes);
        vector::push_back(&mut res, pvk.delta_g2_neg_pc_bytes);
        res
    }

    /// A `PublicProofInputs` wrapper around its serialized bytes.
    struct PublicProofInputs has store, copy, drop {
        bytes: vector<u8>,
    }

    /// Creates a `PublicProofInputs` wrapper from bytes.
    public fun public_proof_inputs_from_bytes(bytes: vector<u8>): PublicProofInputs {
        PublicProofInputs { bytes }
    }

    /// A `ProofPoints` wrapper around the serialized form of three proof points.
    struct ProofPoints has store, copy, drop {
        bytes: vector<u8>
    }

    /// Creates a Groth16 `ProofPoints` from bytes.
    public fun proof_points_from_bytes(bytes: vector<u8>): ProofPoints {
        ProofPoints { bytes }
    }

    /// @param veriyfing_key: An Arkworks canonical compressed serialization of a verifying key.
    ///
    /// Returns four vectors of bytes representing the four components of a prepared verifying key.
    /// This step computes one pairing e(P, Q), and binds the verification to one particular proof statement.
    /// This can be used as inputs for the `verify_groth16_proof` function.
    public native fun prepare_verifying_key(verifying_key: &vector<u8>): PreparedVerifyingKey;

    /// @param prepared_verifying_key: Consists of four vectors of bytes representing the four components of a prepared verifying key.
    /// @param public_proof_inputs: Represent inputs that are public.
    /// @param proof_points: Represent three proof points.
    ///
    /// Returns a boolean indicating whether the proof is valid.
    public fun verify_groth16_proof(prepared_verifying_key: &PreparedVerifyingKey, public_proof_inputs: &PublicProofInputs, proof_points: &ProofPoints): bool {
        verify_groth16_proof_internal(
            &prepared_verifying_key.vk_gamma_abc_g1_bytes,
            &prepared_verifying_key.alpha_g1_beta_g2_bytes,
            &prepared_verifying_key.gamma_g2_neg_pc_bytes,
            &prepared_verifying_key.delta_g2_neg_pc_bytes,
            &public_proof_inputs.bytes,
            &proof_points.bytes
        )
    }

    /// Native functions that flattens the inputs into arrays of vectors and passed to the Rust native function.
    public native fun verify_groth16_proof_internal(vk_gamma_abc_g1_bytes: &vector<u8>, alpha_g1_beta_g2_bytes: &vector<u8>, gamma_g2_neg_pc_bytes: &vector<u8>, delta_g2_neg_pc_bytes: &vector<u8>, public_proof_inputs: &vector<u8>, proof_points: &vector<u8>): bool;
}
