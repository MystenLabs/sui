// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A verifier for the Groth16 zk-SNARK over the BLS12-381 construction.
// See https://eprint.iacr.org/2016/260.pdf for details.
module crypto::groth16 {

    use sui::bls12381;
    use sui::group_ops::Element;

    const EInvalidNumberOfPublicInputs: u64 = 0;

    /// A Groth16 proof.
    public struct Proof has drop {
        a: Element<bls12381::G1>,
        b: Element<bls12381::G2>,
        c: Element<bls12381::G1>,
    }

    /// Create a new `Proof`.
    public fun create_proof(a: Element<bls12381::G1>, b: Element<bls12381::G2>, c: Element<bls12381::G1>): Proof {
        Proof {
            a, 
            b, 
            c,
        }
    }

    /// A Groth16 verifying key used to verify a zero-knowledge proof.
    public struct VerifyingKey has store, drop {
        alpha: Element<bls12381::G1>,
        beta: Element<bls12381::G2>,
        gamma: Element<bls12381::G2>,
        gamma_abc: vector<Element<bls12381::G1>>,
        delta: Element<bls12381::G2>,
    }

    /// Create a new `VerifyingKey`.
    public fun create_verifying_key(
        alpha: Element<bls12381::G1>, 
        beta: Element<bls12381::G2>, 
        gamma: Element<bls12381::G2>, 
        gamma_abc: vector<Element<bls12381::G1>>, 
        delta: Element<bls12381::G2>): VerifyingKey {
        VerifyingKey {
            alpha,
            beta,
            gamma,
            gamma_abc,
            delta,
        }
    }

    /// A prepared verifying key. This makes verification faster than using the verifying key directly.
    public struct PreparedVerifyingKey has store, drop {
        alpha_beta: Element<bls12381::GT>,
        gamma_neg: Element<bls12381::G2>,
        gamma_abc: vector<Element<bls12381::G1>>,
        delta_neg: Element<bls12381::G2>,
    }

    /// Create a PreparedVerifyingKey from a VerifyingKey. This only have to be done once.
    public fun prepare(vk: VerifyingKey): PreparedVerifyingKey {
        PreparedVerifyingKey {
            alpha_beta: bls12381::pairing(&vk.alpha, &vk.beta),
            gamma_neg: bls12381::g2_neg(&vk.gamma),
            gamma_abc: vk.gamma_abc,
            delta_neg: bls12381::g2_neg(&vk.delta),
        }
    }

    fun prepare_inputs(vk_gamma_abc: &vector<Element<bls12381::G1>>, public_inputs: &vector<Element<bls12381::Scalar>>): Element<bls12381::G1> {
        let length = public_inputs.length();
        assert!(length + 1 == vk_gamma_abc.length(), EInvalidNumberOfPublicInputs);

        let mut output = vk_gamma_abc[0];
        let mut i = 0;
        while (i < length) {
            output = bls12381::g1_add(&output, &bls12381::g1_mul(&public_inputs[i], &vk_gamma_abc[i + 1]));
            i = i + 1;
        };
        output
    }

    /// Verify a Groth16 proof with some public inputs and a verifying key.
    public fun verify(pvk: &PreparedVerifyingKey, proof: &Proof, public_inputs: &vector<Element<bls12381::Scalar>>): bool {
        let prepared_inputs = prepare_inputs(&pvk.gamma_abc, public_inputs);
        let mut lhs = bls12381::pairing(&proof.a, &proof.b);
        lhs = bls12381::gt_add(&lhs, &bls12381::pairing(&prepared_inputs, &pvk.gamma_neg));
        lhs = bls12381::gt_add(&lhs, &bls12381::pairing(&proof.c, &pvk.delta_neg));
        lhs == pvk.alpha_beta
    }

    #[test]
    fun test_verification() {
        let vk = create_verifying_key(
            bls12381::g1_from_bytes(&x"b58cfc3b0f43d98e7dbe865af692577d52813cb62ef3c355215ec3be2a0355a1ae5da76dd3e626f8a60de1f4a8138dee"),
            bls12381::g2_from_bytes(&x"9047b42915b32ef9dffe3acc0121a1450416e7f9791159f165ab0729d744da3ed82cd4822ca1d7fef35147cfd620b59b0ca09db7dff43aab6c71635ba8f86a83f058e9922e5cdacbe21d0e5e855cf1e776a61b272c12272fe526f5ba3b48d579"),
            bls12381::g2_from_bytes(&x"ad7c5a6cefcae53a3fbae72662c7c04a2f8e1892cb83615a02b32c31247172b7f317489b84e72f14acaf4f3e9ed18141157c6c1464bf15d957227f75a3c550d6d27f295b41a753340c6eec47b471b2cb8664c84f3e9b725325d3fb8afc6b56d0"),
            vector[
                bls12381::g1_from_bytes(&x"b2c9c61ccc28e913284a47c34e60d487869ff423dd574db080d35844f9eddd2b2967141b588a35fa82a278ce39ae6b1a"),
                bls12381::g1_from_bytes(&x"9026ae12d58d203b4fc5dfad4968cbf51e43632ed1a05afdcb2e380ee552b036fbefc7780afe9675bcb60201a2421b2c")
            ],
            bls12381::g2_from_bytes(&x"b1294927d02f8e86ac57c3b832f4ecf5e03230445a9a785ac8d25cf968f48cca8881d0c439c7e8870b66567cf611da0c1734316632f39d3125c8cecca76a8661db91cbfae217547ea1fc078a24a1a31555a46765011411094ec649d42914e2f5"),
        );

        let public_inputs = vector[bls12381::scalar_from_bytes(&x"46722abc81a82d01ac89c138aa01a8223cb239ceb1f02cdaad7e1815eb997ca6")];

        let proof = create_proof(
            bls12381::g1_from_bytes(&x"9913bdcabdff2cf1e7dea1673c5edba5ed6435df2f2a58d6d9e624609922bfa3976a98d991db333812bf6290a590afaa"),
            bls12381::g2_from_bytes(&x"b0265b35af5069593ee88626cf3ba9a0f07699510a25aec3a27048792ab83b3467d6b814d1c09c412c4dcd7656582e6607b72915081c82794ccedf643c27abace5b23a442079d8dcbd0d68dd697b8e0b699a1925a5f2c77f5237efbbbeda3bd0"),
            bls12381::g1_from_bytes(&x"b1237cf48ca7aa98507e826aac336b9e24f14133de1923fffac602a1203b795b3037c4c94e7246bacee7b2757ae912e5"),
        );

        assert!(verify(&vk.prepare(), &proof, &public_inputs), 0);
    }
}
