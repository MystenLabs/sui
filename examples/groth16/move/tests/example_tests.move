// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[test_only]
module example::example_tests;

use sui::groth16;

#[test]
fun test_move() {
    let pvk = groth16::prepare_verifying_key(&groth16::bn254(), &x"9559a9430cc529483c918db02dd7be4fa59120bca246bc9efcdc9c7e5ad7df84aca86600d0f608d0d23baedf750176fc9b3e50872e2e85bab4ab03e23f7c3d04a2b91aa1bdd80c0a69b43f481a640d47e13b6103040896f28bda039e75c9e00fa719a6138a6fdf7d51feca0f36b4752cae67d3fc6073f860fba34ed825771f03365a8cd86b0f79d680e62e17be1f9b108fdab820046060d7b9f5a8fb5c31d1af7b0d2014d5c1b3a3cd3fe4b3a1da3edab11644a355dec1a5f4bc9f284f441b2a79af04d9a9ffa8078ca3ffc09a0d9f1fa6c0d7e61b397d4fdbac5132f118810502000000000000002bb18134320f28f5cb213771b477439d16a5a2063535baf69062f98aa97002a9710154638a0ab5ab2bd5d241e05fd43bc06217b62ff18820e4ca55b5ba1305a3");
    
    let proof_points = groth16::proof_points_from_bytes(x"60586063ff131ab34fc0dd672b0b93f99c1862d68a3d15f4deb975a7dd7ca88462ae976fe6849298f5f6b91023243c179d39da94ebedf08a9da4ee9a357e4015afdfc0e3624c608cb7cd580c3268ffe9e66a229f731f288b9ed56b615dbba592eb26f14f85207e16956b12172aa66b6c1067193413dc48f4c7330ba960dd0790");
    
    let public_inputs = groth16::public_proof_inputs_from_bytes(x"0100000001000000000000000000000000000000000000000000000000000000");
    assert!(groth16::verify_groth16_proof(&groth16::bn254(), &pvk, &public_inputs, &proof_points));
}

