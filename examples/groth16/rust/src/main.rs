// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ark_bn254::{Bn254, Fr};
use ark_circom::CircomBuilder;
use ark_circom::CircomConfig;
use ark_groth16::{prepare_verifying_key, Groth16};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;

#[tokio::main]
async fn main() {
    // Load the WASM and R1CS for witness and proof generation
    let cfg =
        CircomConfig::<Fr>::new("../circuit/main_js/main.wasm", "../circuit/main.r1cs").unwrap();
    let mut builder = CircomBuilder::new(cfg);

    // Private inputs: A factorisation of a number
    builder.push_input("a", 641);
    builder.push_input("b", 6_700_417);

    let circuit = builder.setup();

    // Generate a random proving key. WARNING: This is not secure. A proving key generated from a ceremony should be used in production.
    let mut rng = ark_std::rand::thread_rng();
    let pk =
        Groth16::<Bn254>::generate_random_parameters_with_reduction(circuit, &mut rng).unwrap();

    let circuit = builder.build().unwrap();
    let public_inputs = circuit.get_public_inputs().unwrap();

    // Create proof
    let proof = Groth16::<Bn254>::prove(&pk, circuit, &mut rng).unwrap();

    // Verify proof
    let pvk = prepare_verifying_key(&pk.vk);
    let verified =
        Groth16::<Bn254>::verify_with_processed_vk(&pvk, &public_inputs, &proof).unwrap();
    assert!(verified);

    // Print verifying key
    let mut pk_bytes = Vec::new();
    pk.vk.serialize_compressed(&mut pk_bytes).unwrap();
    println!("Verifying key: {}", hex::encode(pk_bytes));

    // Print proof
    let mut proof_serialized = Vec::new();
    proof.serialize_compressed(&mut proof_serialized).unwrap();
    println!("Proof: {}", hex::encode(proof_serialized));

    // Print public inputs. Note that they are concatenated.
    let mut public_inputs_serialized = Vec::new();
    public_inputs.iter().for_each(|input| {
        input
            .serialize_compressed(&mut public_inputs_serialized)
            .unwrap();
    });
    println!("Public inputs: {}", hex::encode(public_inputs_serialized));
}
