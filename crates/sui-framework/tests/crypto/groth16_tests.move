// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::groth16_tests {
    use sui::groth16;
    use std::vector;
      #[test]
    fun test_prepare_verifying_key() {
        let vk = x"88c841f7013e91bc61827a64da5f372842e9be522513983253c2a9275e434d93130d100c4b8124fe55dc0dc1ef45918b4d07f0c8b3873b170af258021e71a4dc507aca4fdeafd5dc2f3ee8598117863a57fc25efc408d4227b22e60e8d84bb146e97637d3fbba78a8641f44cfff82cb894472075a6d3515c54ce9fa2ca186f2d5780747b5b7c85e88da7be1a815a3904f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd520afdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b5850200000000000000f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let arr = groth16::pvk_to_bytes(groth16::prepare_verifying_key(&vk));

        let expected_vk_bytes = x"f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let expected_alpha_bytes = x"12168aa38a1ae0360550d0541002b024057ab689d45ce809f8ea36d5286eca9e2f18e70924ac69dcd432228a18036b146aa75a5c17430751f844f686c8ba210c7736adb1851f7afac7fbbc4ac78a01c7ca4508e3d45b5dd31e875c99b0c9d20004f4b3ad8e3c8842b6adc9c3797e3083a31b1ffe654dd4466743cd943b7d3185588a2d81da5f20b36593157c2429b21835964abb93670c81f4a9f230556dcedcc87a5c365613820e225225a650ba7d5a8d283db8317529b37297979ad7576405b26e53f2c162e35557eaf4e59e1b3d456d486291a644fe098f0d29c0435d46e35d114d7357188ed8a8fa26c807fa420e7bff7ce0c2a84a75f189cf6ed039564f36441236720be11bc53850f3700491f50430fe4729676564128f0bf326e67a0038975b396c6fd12c0cd8be75e5985e2841005640b6104b4e1e9817dd3b44e51aa4b0972489ad999bb8143a4e833110057ba32d1ff91c6707b07eab0605b9d6a2745aead54f16a968a4122fa8ca871b70a100b5fd854d4473ec7b519c04547f14b9aba6701e54e737161fc154cc3751f995c0c33d7ef74b893e6bc5514891d73af5543c4ed463e4aebe6cbbd97390bf0bf72075a0649e01a65fa2b7198bedac38406864dc780cb8789df0cb09cf532201d589bc40f84bf6a5816ccbd31ea85d0cf2e06c26037d6970caee38b507450bef282c40366bb4506408f17e331fde3211c0cb021c7858ba83e6a1f1d24bdf550b884d857ff0355ad83cd01346c62dca7197b4d54288ebc982d8228a8403e9a8bd95ef98775bf9c40004e2b5de3e663212";
        let expected_gamma_bytes = x"f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd528a";
        let expected_delta_bytes = x"fdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b505";

        let delta_bytes = vector::pop_back(&mut arr);
        assert!(delta_bytes == expected_delta_bytes, 0);

        let gamma_bytes = vector::pop_back(&mut arr);
        assert!(gamma_bytes == expected_gamma_bytes, 0);

        let alpha_bytes = vector::pop_back(&mut arr);
        assert!(alpha_bytes == expected_alpha_bytes, 0);

        let vk_bytes = vector::pop_back(&mut arr);
        assert!(vk_bytes == expected_vk_bytes, 0);
   }

    #[test]
    #[expected_failure(abort_code = groth16::EInvalidVerifyingKey)]
    fun test_prepare_verifying_key_invalid() {
        let invalid_vk = x"";
        groth16::prepare_verifying_key(&invalid_vk);
    }

    #[test]
    fun test_verify_groth_16_proof() {
        // Success case.
        let vk_bytes = x"f675d896123954189d34681ef5ce47b5e3260247e4ea6817f19c410b9f6fe3deb086e165056c02216a0e12114a9b410d76805e906193c8cd44b02fcd9d9b34fdb6b275ef5c13e7056fb61aa1870409b8020810f9b29aab6b339fa3f853c0e103";
        let alpha_bytes = x"12168aa38a1ae0360550d0541002b024057ab689d45ce809f8ea36d5286eca9e2f18e70924ac69dcd432228a18036b146aa75a5c17430751f844f686c8ba210c7736adb1851f7afac7fbbc4ac78a01c7ca4508e3d45b5dd31e875c99b0c9d20004f4b3ad8e3c8842b6adc9c3797e3083a31b1ffe654dd4466743cd943b7d3185588a2d81da5f20b36593157c2429b21835964abb93670c81f4a9f230556dcedcc87a5c365613820e225225a650ba7d5a8d283db8317529b37297979ad7576405b26e53f2c162e35557eaf4e59e1b3d456d486291a644fe098f0d29c0435d46e35d114d7357188ed8a8fa26c807fa420e7bff7ce0c2a84a75f189cf6ed039564f36441236720be11bc53850f3700491f50430fe4729676564128f0bf326e67a0038975b396c6fd12c0cd8be75e5985e2841005640b6104b4e1e9817dd3b44e51aa4b0972489ad999bb8143a4e833110057ba32d1ff91c6707b07eab0605b9d6a2745aead54f16a968a4122fa8ca871b70a100b5fd854d4473ec7b519c04547f14b9aba6701e54e737161fc154cc3751f995c0c33d7ef74b893e6bc5514891d73af5543c4ed463e4aebe6cbbd97390bf0bf72075a0649e01a65fa2b7198bedac38406864dc780cb8789df0cb09cf532201d589bc40f84bf6a5816ccbd31ea85d0cf2e06c26037d6970caee38b507450bef282c40366bb4506408f17e331fde3211c0cb021c7858ba83e6a1f1d24bdf550b884d857ff0355ad83cd01346c62dca7197b4d54288ebc982d8228a8403e9a8bd95ef98775bf9c40004e2b5de3e663212";
        let gamma_bytes = x"f63b997d4f3d45ed3e20e5cb0e17b0b962b62e9d64d5bc825fe571ffc15f98b10605758eaf440fe16513386c086c9e0b0bea1c30f8f8bf1667dcc47514a9adc4cd1b2d854c0fd2291e0140b7f6d34f31c3cb6c8ee635b9394821369154dd528a";
        let delta_bytes = x"fdaacd48da6deedb190f27f59d9740c3607bbfcb2c0f8a590b4ee9071a9bda9532217f89aab2fd4e2d505f47cc113c00618849268b140fab6be405649a2d1d074983183287b8ee7a73c4dbb2ab4e7ba3bab7fa005a055a3dd26b4787fe11b505";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        let inputs_bytes = x"4af76d91d4bc9a3973c15e3aeb574f0f64547b838f950af35db97f0705c4214b";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"cf4321ae78c61edef79dd0c4b2e6c4a48c24914e9b2b8e6aa9ff0c5e141beae84b80b49510beb90218a76cedb39dcc97fc309ed6d911c8ad65975e081b51c089c95a70ea6dd516ca09c9a59c4ee4f624d645ecbc9fac020194cc0962ab4f040f4d765b0e69014a47bc9f1b06e0ba818bfff2a51f424e3eba325b514e0da88c4e0aae399231bfd8daa29536cf2ddca0986f88147b749d1be59437610aaf7d0c34b200f58e2d2a93f4ecd14208a314583804dd2a3bc283ec00de01ecf789384507";
        let proof = groth16::proof_points_from_bytes(proof_bytes);

        assert!(groth16::verify_groth16_proof(&pvk, &inputs, &proof) == true, 0);

        // Invalid prepared verifying key.
        vector::pop_back(&mut vk_bytes);
        let invalid_pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);
        assert!(groth16::verify_groth16_proof(&invalid_pvk, &inputs, &proof) == false, 0);

        // Invalid public inputs bytes.
        let invalid_inputs = groth16::public_proof_inputs_from_bytes(x"cf");
        assert!(groth16::verify_groth16_proof(&pvk, &invalid_inputs, &proof) == false, 0);

        // Invalid proof bytes.
        let invalid_proof = groth16::proof_points_from_bytes(x"4a");
        assert!(groth16::verify_groth16_proof(&pvk, &inputs, &invalid_proof) == false, 0);
    }
}