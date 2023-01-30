// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A basic groth16 contract that verifies a Groth16 proof of knowleddge of the preimage of Black2s hash. 
// 1. Prepare a verifying key
// 2. Submit the prepared verifying key, the public  
module math::groth16_blake2s {
    use sui::groth16;
    use sui::event;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;
    use std::vector;

    /// Event on whether the groth16 proof is verified
    struct VerifiedEvent has copy, drop {
        is_verified: bool,
    }

    public entry fun prepare_verifying_key(verifying_key: vector<u8>): PreparedVerifyingKey {

    }

    public entry fun verify_groth16_proof(pvk: &PreparedVerifyingKey, public_proof_inputs: &PublicProofInputs, proof_points: &ProofPoints) {
        event::emit(VerifiedEvent {is_verified: groth16::verify_groth16_proof(&pvk, &public_proof_inputs, &hashed_msg)});
    }
}
