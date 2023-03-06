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
        let vk_bytes = x"ada3c24e8c2e63579cc03fd1f112a093a17fc8ab0ff6eee7e04cab7bf8e03e7645381f309ec113309e05ac404c77ac7c8585d5e4328594f5a70a81f6bd4f29073883ee18fd90e2aa45d0fc7376e81e2fdf5351200386f5732e58eb6ff4d318dc";
        let alpha_bytes = x"8b0f85a9e7d929244b0af9a35af10717bd667b6227aae37a6d336e815fb0d850873e0d87968345a493b2d31aa8aa400d9820af1d35fa862d1b339ea1f98ac70db7faa304bff120a151a1741d782d08b8f1c1080d4d2f3ebee63ac6cadc666605be306de0973be38fbbf0f54b476bbb002a74ff9506a2b9b9a34b99bfa7481a84a2c9face7065c19d7069cc5738c5350b886a5eeebe656499d2ffb360afc7aff20fa9ee689fb8b46863e90c85224e8f597bf323ad4efb02ee96eb40221fc89918a2c740eabd2886476c7f247a3eb34f0106b3b51cf040e2cdcafea68b0d8eecabf58b5aa2ece3d86259cf2dfa3efab1170c6eb11948826def533849b68335d76d60f3e16bb5c629b1c24df2bdd1a7f13c754d7fe38617ecd7783504e4615e5c13168185cc08de8d63a0f7032ab7e82ff78cf0bc46a84c98f2d95bb5af355cbbe525c44d5c1549c169dfe119a219dbf9038ec73729d187bd0e3ed369e4a2ec2be837f3dcfd958aea7110627d2c0192d262f17e722509c17196005b646a556cf010ef9bd2a2a9b937516a5ecdee516e77d14278e96bc891b630fc833dda714343554ae127c49460416430b7d4f048d08618058335dec0728ad37d10dd9d859c385a38673e71cc98e8439da0accc29de5c92d3c3dc98e199361e9f7558e8b0a2a315ccc5a72f54551f07fad6f6f4615af498aba98aea01a13a4eb84667fd87ee9782b1d812a03f8814f042823a7701238d0fec1e7dec2a26ffea00330b5c7930e95138381435d2a59f51313a48624e30b0a685e357874d41a0a19d83f7420c1d9c04";
        let gamma_bytes = x"b675d1ff988116d1f2965d3c0c373569b74d0a1762ea7c4f4635faa5b5a8fa198a2a2ce6153f390a658dc9ad01a415491747e9de7d5f493f59cf05a52eb46eaac397ffc47aef1396cf0d8b75d0664077ea328ad6b63284b42972a8f11c523a60";
        let delta_bytes = x"8229cb9443ef1fb72887f917f500e2aef998717d91857bcb92061ecd74d1d24c2b2b282736e8074e4316939b4c9853c117aa08ed49206860d648818b2cccb526585f5790161b1730d39c73603b482424a27bba891aaa6d99f3025d3df2a6bd42";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        let inputs_bytes = x"440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"a29981304df8e0f50750b558d4de59dbc8329634b81c986e28e9fff2b0faa52333b14a1f7b275b029e13499d1f5dd8ab955cf5fa3000a097920180381a238ce12df52207597eade4a365a6872c0a19a39c08a9bfb98b69a15615f90cc32660180ca32e565c01a49b505dd277713b1eae834df49643291a3601b11f56957bde02d5446406d0e4745d1bd32c8ccb8d8e80b877712f5f373016d2ecdeebb58caebc7a425b8137ebb1bd0c5b81c1d48151b25f0f24fe9602ba4e403811fb17db6f14";
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