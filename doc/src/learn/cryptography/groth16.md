---
title: Zero-knowledge proof verification (Groth16)
---

## Overview

A zero-knowledge proof allows a prover to prove that a statement is true without revealing any information about the inputs, for example a prover can prove that they know the solution to a sudoku puzzle without revealing the solution.

Zk-SNARKs (Zero-Knowledge Succinct Non-Interactive Argument of Knowledge) are a family of zero-knowledge proofs which are non-interactive, have succinct proof size and efficient verification time. 
An important and widely used variant of them is pairing-based zk-SNARKs, of which the [Groth16](https://eprint.iacr.org/2016/260.pdf) proof system is one of the most efficient and widely used.

The Move API in Sui allows users to verify any statement that can be expressed in a NP-complete language can be verified efficiently using Groth16 zk-SNARKs over either the BN254 or BLS12-381 elliptic curve constructions.

There are high-level languages for expressing these statements, such as [Circom](https://docs.circom.io) which will be used in the example below.

Note that Groth16 requires a trusted setup for each circuit to generate the verification key. The API is not pinning any particular verification key. and each user can generate their own parameters or use an existing verification to their dapps.

## Usage

The following example demonstrates how to create a Groth16 proof from a statement written in Circom and then verify it using the Sui Move API. The API currently supports up to eight public inputs. 

## Create circuit
The proof demonstrates that we know a secret input to a hash function which gives a certain public output.
```circom
pragma circom 2.1.5;

include "node_modules/circomlib/circuits/poseidon.circom";

template Main() {
    component poseidon = Poseidon(1);
    signal input in;
    signal output digest;
    poseidon.inputs[0] <== in;
    digest <== poseidon.out;
}

component main = Main();
```
We use the [Poseidon hash function](https://www.poseidon-hash.info) which is a ZK-friendly hash function. Assuming that the [circom compiler has been installed](https://docs.circom.io/getting-started/installation/), the above circuit is compiled using the following command:
```shell
circom main.circom --r1cs --wasm 
```
This outputs the constraints in R1CS format and the circuit in Wasm format.

## Generate proof
In order to generate a proof that can be verified in Sui, we need to generate a witness. Here, we show an example of how to do this using Arkworks' [ark-circom](https://github.com/gakonst/ark-circom) Rust library. The code below constructs a witness for the circuit and generates a proof for it for a given input. Finally, it verifies that the proof is correct.

```rust
use ark_bn254::Bn254;
use ark_circom::CircomBuilder;
use ark_circom::CircomConfig;
use ark_groth16::Groth16;
use ark_snark::SNARK;

fn main() {
    // Load the WASM and R1CS for witness and proof generation
    let cfg = CircomConfig::<Bn254>::new("main.wasm", "main.r1cs").unwrap();

    // Insert our secret inputs as key value pairs. We insert a single input, namely the input to the hash function.
    let mut builder = CircomBuilder::new(cfg);
    builder.push_input("in", 7);

    // Create an empty instance for setting it up
    let circom = builder.setup();

    // WARNING: The code below is just for debugging, and should instead use a verification key generated from a trusted setup.
    // See for example https://docs.circom.io/getting-started/proving-circuits/#powers-of-tau.
    let mut rng = rand::thread_rng();
    let params =
        Groth16::<Bn254>::generate_random_parameters_with_reduction(circom, &mut rng).unwrap();

    let circom = builder.build().unwrap();

    // There's only one public input, namely the hash digest.
    let inputs = circom.get_public_inputs().unwrap();

    // Generate the proof
    let proof = Groth16::<Bn254>::prove(&params, circom, &mut rng).unwrap();

    // Check that the proof is valid
    let pvk = Groth16::<Bn254>::process_vk(&params.vk).unwrap();
    let verified = Groth16::<Bn254>::verify_with_processed_vk(&pvk, &inputs, &proof).unwrap();
    assert!(verified);
}
```
The proof shows that we know an input (7) which, when hashed with the Poseidon hash function, gives a certain output (which in this case is `inputs[0] = 7061949393491957813657776856458368574501817871421526214197139795307327923534`).

## Verification in Sui
The API in Sui for verifying a proof expects a special processed verification key, where only a subset of the values are used. Ideally, we would like to compute this prepared verification key only
once per circuit. This processing can be done with the Sui Move API by calling `sui::groth16::prepare_verifying_key` with a serialization of the `params.vk` value above.
The output of the `prepare_verifying_key` function is a vector with four byte arrays, which corresponds to the `vk_gamma_abc_g1_bytes`, `alpha_g1_beta_g2_bytes`, `gamma_g2_neg_pc_bytes`, `delta_g2_neg_pc_bytes`. 

In order to verify a proof, we also need two more inputs, `proof_inputs_bytes` and `proof_points_bytes`, which contains the public inputs and the proof respectively. These are serializations of the `inputs` and `proof` values from the example above, which in Rust can be computed as follows:
```rust
let mut proof_inputs_bytes = Vec::new();
inputs.serialize_compressed(&mut proof_inputs_bytes).unwrap();

let mut proof_points_bytes = Vec::new();
proof.a.serialize_compressed(&mut proof_points_bytes).unwrap();
proof.b.serialize_compressed(&mut proof_points_bytes).unwrap();
proof.c.serialize_compressed(&mut proof_points_bytes).unwrap();
```
Below is an example smart contract which prepares a verification key and verify the corresponding proof. Note that this example uses the BN254 elliptic curve construction which is given as the first parameter to the `prepare_verifying_key` and `verify_groth16_proof` functions, so if the BLS12-381 construction should be used instead, the `bls12381` function should be called instead.
```move
module test::groth16_test {
    use sui::groth16;
    use sui::event;

    /// Event on whether the proof is verified
    struct VerifiedEvent has copy, drop {
        is_verified: bool,
    }

    public entry fun verify_proof(vk: vector<u8>, public_inputs_bytes: vector<u8>, proof_points_bytes: vector<u8>) {
        let pvk = groth16::prepare_verifying_key(&groth16::bn254(), &vk);
        let public_inputs = groth16::public_proof_inputs_from_bytes(public_inputs_bytes);
        let proof_points = groth16::proof_points_from_bytes(proof_points_bytes);
        event::emit(VerifiedEvent {is_verified: groth16::verify_groth16_proof(&groth16::bn254(), &pvk, &public_inputs, &proof_points)});
    }
}
```