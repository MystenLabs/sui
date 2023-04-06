// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::groth16_tests {
    use sui::groth16;
    use std::vector;
    use sui::groth16::{bls12381, bn254};

    #[test]
    fun test_prepare_verifying_key_bls12381() {
        let vk = x"a84d039ad1ae98eeeee4c8ba9af9b6c5d1cfcb98c3fc92ccfcebd77bcccffa1d170d39da29e9b4aa83b98680cb90bb25946b2b70f9e3565510c5361d5d65cb458a0b3177d612dd340b8f8f8493c2772454e3e8f577a3f77865df851d1a159b800c2ec5bae889029fc419678e83dee900465d60e7ef26f614940e719c6f7c0c7db57464fa0481a93c18d52cb2fbf8dcf0a398b153643614fc1071a54e288edb6402f1d9e00d3408c76d95c16885cc992dff5c6ebee3b739cb22359ab2d126026a1626c43ea7b898a7c1d2904c1bd4bbce5d0b1b16fab8535a52d1b08a5217df2e912ee1b0f4140892afa31d479f78dfbc82ab58a209ad00df6c86ab14841e8daa7a380a6853f28bacf38aad9903b6149fff4b119dea16de8aa3e5050b9d563a01009e061a950c233f66511c8fae2a8c58503059821df7f6defbba8f93d26e412cc07b66a9f3cdd740cce5c8488ce94fc8020000000000000081aabea18713222ac45a6ef3208a09f55ce2dde8a11cc4b12788be2ae77ae318176d631d36d80942df576af651b57a31a95f2e9bcaebbb53a588251634715599f7a7e9d51fe872fe312edf0b39d98f0d7f8b5554f96f759c041ea38b4b1e5e19";
        let arr = groth16::pvk_to_bytes(groth16::prepare_verifying_key(&bls12381(), &vk));

        let expected_vk_bytes = x"81aabea18713222ac45a6ef3208a09f55ce2dde8a11cc4b12788be2ae77ae318176d631d36d80942df576af651b57a31a95f2e9bcaebbb53a588251634715599f7a7e9d51fe872fe312edf0b39d98f0d7f8b5554f96f759c041ea38b4b1e5e19";
        let expected_alpha_bytes = x"097ca8074c7f1d661e25d70fc2e6f14aa874dabe3d8a5d7751a012a737d30b59fc0f5f6d4ce0ea6f6c4562912dfb2a1442df06f9f0b8fc2d834ca007c8620823926b2fc09367d0dfa9b205a216921715e13deedd93580c77cae413cbb83134051cb724633c58759c77e4eda4147a54b03b1f443b68c65247166465105ab5065847ae61ba9d8bdfec536212b0dadedc042dab119d0eeea16349493a4118d481761b1e75f559fbad57c926d599e81d98dde586a2cfcc37b49972e2f9db554e5a0ba56bec2d57a8bfed629ae29c95002e3e943311b7b0d1690d2329e874b179ce5d720bd7c5fb5a2f756b37e3510582cb0c0f8fc8047305fc222c309a5a8234c5ff31a7b311aabdcebf4a43d98b69071a9e5796372146f7199ba05f9ca0a3d14b0c421e7f1bd02ac87b365fd8ce992c0f87994d0ca66f75c72fed0ce94ca174fcb9e5092f0474e07e71e9fd687b3daa441193f264ca2059760faa9c5ca5ef38f6ecefef2ac7d8c47df67b99c36efa64f625fe3f55f40ad1865abbdf2ff4c3fc3a162e28b953f6faec70a6a61c76f4dca1eecc86544b88352994495ae7fc7a77d387880e59b2357d9dd1277ae7f7ee9ba00b440e0e6923dc3971de9050a977db59d767195622f200f2bf0d00e4a986e94a6932627954dd2b7da39b4fcb32c991a0190bdc44562ad83d34e0af7656b51d6cde03530b5d523380653130b87346720ad6dd425d8133ffb02f39a95fc70e9707181ecb168bd8d2d0e9e85e262255fecab15f1ada809ecbefa42a7082fa7326a1d494261a8954fe5b215c5b761fb10b7f18";
        let expected_gamma_bytes = x"8398b153643614fc1071a54e288edb6402f1d9e00d3408c76d95c16885cc992dff5c6ebee3b739cb22359ab2d126026a1626c43ea7b898a7c1d2904c1bd4bbce5d0b1b16fab8535a52d1b08a5217df2e912ee1b0f4140892afa31d479f78dfbc";
        let expected_delta_bytes = x"a2ab58a209ad00df6c86ab14841e8daa7a380a6853f28bacf38aad9903b6149fff4b119dea16de8aa3e5050b9d563a01009e061a950c233f66511c8fae2a8c58503059821df7f6defbba8f93d26e412cc07b66a9f3cdd740cce5c8488ce94fc8";

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
    fun test_prepare_verifying_key_invalid_bls12381() {
        let invalid_vk = x"";
        groth16::prepare_verifying_key(&bls12381(), &invalid_vk);
    }

    #[test]
    fun test_verify_groth_16_proof_bls12381() {
        let curve = bls12381();

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

        assert!(groth16::verify_groth16_proof(&curve, &pvk, &inputs, &proof) == true, 0);

        // Invalid prepared verifying key.
        vector::pop_back(&mut vk_bytes);
        let invalid_pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);
        assert!(groth16::verify_groth16_proof(&curve, &invalid_pvk, &inputs, &proof) == false, 0);

        // Invalid public inputs bytes.
        let invalid_inputs = groth16::public_proof_inputs_from_bytes(x"cf");
        assert!(groth16::verify_groth16_proof(&curve, &pvk, &invalid_inputs, &proof) == false, 0);

        // Invalid proof bytes.
        let invalid_proof = groth16::proof_points_from_bytes(x"4a");
        assert!(groth16::verify_groth16_proof(&curve, &pvk, &inputs, &invalid_proof) == false, 0);
    }

    #[test]
    #[expected_failure(abort_code = groth16::ETooManyPublicInputs)]
    fun test_too_many_public_inputs_bls12381() {
        let curve = bls12381();

        let vk_bytes = x"ada3c24e8c2e63579cc03fd1f112a093a17fc8ab0ff6eee7e04cab7bf8e03e7645381f309ec113309e05ac404c77ac7c8585d5e4328594f5a70a81f6bd4f29073883ee18fd90e2aa45d0fc7376e81e2fdf5351200386f5732e58eb6ff4d318dc";
        let alpha_bytes = x"8b0f85a9e7d929244b0af9a35af10717bd667b6227aae37a6d336e815fb0d850873e0d87968345a493b2d31aa8aa400d9820af1d35fa862d1b339ea1f98ac70db7faa304bff120a151a1741d782d08b8f1c1080d4d2f3ebee63ac6cadc666605be306de0973be38fbbf0f54b476bbb002a74ff9506a2b9b9a34b99bfa7481a84a2c9face7065c19d7069cc5738c5350b886a5eeebe656499d2ffb360afc7aff20fa9ee689fb8b46863e90c85224e8f597bf323ad4efb02ee96eb40221fc89918a2c740eabd2886476c7f247a3eb34f0106b3b51cf040e2cdcafea68b0d8eecabf58b5aa2ece3d86259cf2dfa3efab1170c6eb11948826def533849b68335d76d60f3e16bb5c629b1c24df2bdd1a7f13c754d7fe38617ecd7783504e4615e5c13168185cc08de8d63a0f7032ab7e82ff78cf0bc46a84c98f2d95bb5af355cbbe525c44d5c1549c169dfe119a219dbf9038ec73729d187bd0e3ed369e4a2ec2be837f3dcfd958aea7110627d2c0192d262f17e722509c17196005b646a556cf010ef9bd2a2a9b937516a5ecdee516e77d14278e96bc891b630fc833dda714343554ae127c49460416430b7d4f048d08618058335dec0728ad37d10dd9d859c385a38673e71cc98e8439da0accc29de5c92d3c3dc98e199361e9f7558e8b0a2a315ccc5a72f54551f07fad6f6f4615af498aba98aea01a13a4eb84667fd87ee9782b1d812a03f8814f042823a7701238d0fec1e7dec2a26ffea00330b5c7930e95138381435d2a59f51313a48624e30b0a685e357874d41a0a19d83f7420c1d9c04";
        let gamma_bytes = x"b675d1ff988116d1f2965d3c0c373569b74d0a1762ea7c4f4635faa5b5a8fa198a2a2ce6153f390a658dc9ad01a415491747e9de7d5f493f59cf05a52eb46eaac397ffc47aef1396cf0d8b75d0664077ea328ad6b63284b42972a8f11c523a60";
        let delta_bytes = x"8229cb9443ef1fb72887f917f500e2aef998717d91857bcb92061ecd74d1d24c2b2b282736e8074e4316939b4c9853c117aa08ed49206860d648818b2cccb526585f5790161b1730d39c73603b482424a27bba891aaa6d99f3025d3df2a6bd42";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        let inputs_bytes = x"440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849440758042e68b76a376f2fecf3a5a8105edb194c3e774e5a760140305aec8849";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"a29981304df8e0f50750b558d4de59dbc8329634b81c986e28e9fff2b0faa52333b14a1f7b275b029e13499d1f5dd8ab955cf5fa3000a097920180381a238ce12df52207597eade4a365a6872c0a19a39c08a9bfb98b69a15615f90cc32660180ca32e565c01a49b505dd277713b1eae834df49643291a3601b11f56957bde02d5446406d0e4745d1bd32c8ccb8d8e80b877712f5f373016d2ecdeebb58caebc7a425b8137ebb1bd0c5b81c1d48151b25f0f24fe9602ba4e403811fb17db6f14";
        let proof = groth16::proof_points_from_bytes(proof_bytes);

        groth16::verify_groth16_proof(&curve, &pvk, &inputs, &proof);
    }

    #[test]
    fun test_prepare_verifying_key_bn254() {
        let vk = x"53d75f472c207c7fcf6a34bc1e50cf0d7d2f983dd2230ffcaf280362d162c3871cae3e4f91b77eadaac316fe625e3764fb39af2bb5aa25007e9bc6b116f6f02f597ad7c28c4a33da5356e656dcef4660d7375973fe0d7b6dc642d51f16b6c8806030ca5b462a3502d560df7ff62b7f1215195233f688320de19e4b3a2a2cb6120ae49bcc0abbd3cbbf06b29b489edbf86e3b679f4e247464992145f468e3c08db41e5e09002a7170cb4cc56ae96b152d17b6b0d1b9333b41f2325c3c8a9d2e2df98f8e2315884fae52b3c6bb329df0359daac4eff4d2e7ce729078b10d79d42f02000000000000001dcc52e058148a622c51acfdee6e181252ec0e9717653f0be1faaf2a68222e0dd2ccf4e1e8b088efccfdb955a1ff4a0fd28ae2ccbe1a112449ddae8738fb40b0";
        let arr = groth16::pvk_to_bytes(groth16::prepare_verifying_key(&bn254(), &vk));

        let expected_vk_bytes = x"1dcc52e058148a622c51acfdee6e181252ec0e9717653f0be1faaf2a68222e0dd2ccf4e1e8b088efccfdb955a1ff4a0fd28ae2ccbe1a112449ddae8738fb40b0";
        let expected_alpha_bytes = x"61665b255f20b17bbd56b04a9e4d6bf596cb8d578ce5b2a9ccd498e26d394a3071485596cabce152f68889799f7f6b4e94d415c28e14a3aa609e389e344ae72778358ca908efe2349315bce79341c69623a14397b7fa47ae3fa31c6e41c2ee1b6ab50ef5434c1476d9894bc6afee68e0907b98aa8dfa3464cc9a122b247334064ff7615318b47b881cef4869f3dbfde38801475ae15244be1df58f55f71a5a01e28c8fa91fac886b97235fddb726dfc6a916483464ea130b6f82dc602e684b14f5ee655e510a0c1dd6f87b608718cd19d63a914f745a80c8016aa2c49883482aa28acd647cf9ce56446c0330fe6568bc03812b3bda44d804530abc67305f4914a509ecdc30f0b88b1a4a8b11e84856b333da3d86bb669a53dbfcde59511be60d8d5f7c79faa4910bf396ab04e7239d491e0a3bee177e6c9aac0ecbcd09ca850afcd46f25410849cefcfbdac828e7b057d4a732a373aad913d4b767897ba15d0bfcbcbb25bc5f2dae1ea59196ede9666a5c260f054b1a64977666af6a03076409";
        let expected_gamma_bytes = x"6030ca5b462a3502d560df7ff62b7f1215195233f688320de19e4b3a2a2cb6120ae49bcc0abbd3cbbf06b29b489edbf86e3b679f4e247464992145f468e3c00d";
        let expected_delta_bytes = x"b41e5e09002a7170cb4cc56ae96b152d17b6b0d1b9333b41f2325c3c8a9d2e2df98f8e2315884fae52b3c6bb329df0359daac4eff4d2e7ce729078b10d79d4af";

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
    fun test_prepare_verifying_key_invalid_bn254() {
        let invalid_vk = x"";
        groth16::prepare_verifying_key(&bn254(), &invalid_vk);
    }

    #[test]
    fun test_verify_groth_16_proof_bn254() {
        let curve = bn254();

        // Success case.
        let vk_bytes = x"e8324a3242be5193eb38cca8761691ce061e89ce86f1fce8fd7ef40808f12da3c67d9ed5667c841f956e11adbbe240ddf37a1e3a4a890600dc88f608b897898e";
        let alpha_bytes = x"51e6d72cd3b0914dd232653f84e7971d3e5bbcde6b47ff8d6c05277e579f1c1eb2fe30aa252c63950de6ea00dd21a1027f6d130357e47c31fafeca0d31e19406231df42bc11ce376f8cf75135d9074f081c242c31f198d151ec69ec37d67cc2b12542cb306a7823c8b194f13672176c6ee8266b2a0c9f57a5dbdb2278046b511d44e715a3ebe02ec2e1cf493c1b1ada84676e234134a6da5a552f61d4e905e15c0dc58a3414d74304775de5ba8571128f3548d269b51fdc08d5b646fd9157e0a2bc0c4bec5a9a6048d17d1d6cd941b4d459f1de0c7c1d417f33995d2a8dd670b91f0baaccaaf2802100901711885026a5ec97fbbb801000d0d01185651947c1900e336921d07eb16d0e25a2192829540ad5eeb1c498ba9c6316e16807a55dc2b9a7f3dea2e4a2f485ed1295a96d6ca86851842b3a22f83507f93ac66a1dc341d5d22f592527d8ea5c12db16bbabe24b76b3e1baf825c8dcf147be369fd8c5300fd77d0aa8dce730e4e7442c93c4890023f3a266c9fbc90ebbf72825e798c4c00";
        let gamma_bytes = x"240a80664919b9f7490209cff12bfd81c32c272607dc004661c792082cbe282ef826f56a3822ebd72345f86c7ee9872e23f10d1f2dbf43f8aca5dc2ceb5388a5";
        let delta_bytes = x"f755df8c90edab48ac5adafef6a5a461902217f392e3aa4c34c0462b700c18164f79018778755980d491647de11ecc51fda2cc17171c4b44485ec37ccd23a69b";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        let inputs_bytes = x"3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"dd2ef02e57d6a282df6b7f36c134ab7e55c2e04c5b8cbd7831be18e0e7224623ae8bd6c41637c10cbd02f5e68de6394461f417895ddd264d6f0ddacf68c6cd02feb8881f0efa599139a6faf4223dd8743777c4346cba52322eb466af96f2be9f813af1450f84d6f8029804f60cac1add70ad1a3d4226404f84f4022dc18caa0f";
        let proof = groth16::proof_points_from_bytes(proof_bytes);

        assert!(groth16::verify_groth16_proof(&curve, &pvk, &inputs, &proof) == true, 0);

        // Invalid prepared verifying key.
        vector::pop_back(&mut vk_bytes);
        let invalid_pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);
        assert!(groth16::verify_groth16_proof(&curve, &invalid_pvk, &inputs, &proof) == false, 0);

        // Invalid public inputs bytes.
        let invalid_inputs = groth16::public_proof_inputs_from_bytes(x"cf");
        assert!(groth16::verify_groth16_proof(&curve, &pvk, &invalid_inputs, &proof) == false, 0);

        // Invalid proof bytes.
        let invalid_proof = groth16::proof_points_from_bytes(x"4a");
        assert!(groth16::verify_groth16_proof(&curve, &pvk, &inputs, &invalid_proof) == false, 0);
    }

    #[test]
    #[expected_failure(abort_code = groth16::ETooManyPublicInputs)]
    fun test_too_many_public_inputs_bn254() {
        let curve = bn254();

        let vk_bytes = x"e8324a3242be5193eb38cca8761691ce061e89ce86f1fce8fd7ef40808f12da3c67d9ed5667c841f956e11adbbe240ddf37a1e3a4a890600dc88f608b897898e";
        let alpha_bytes = x"51e6d72cd3b0914dd232653f84e7971d3e5bbcde6b47ff8d6c05277e579f1c1eb2fe30aa252c63950de6ea00dd21a1027f6d130357e47c31fafeca0d31e19406231df42bc11ce376f8cf75135d9074f081c242c31f198d151ec69ec37d67cc2b12542cb306a7823c8b194f13672176c6ee8266b2a0c9f57a5dbdb2278046b511d44e715a3ebe02ec2e1cf493c1b1ada84676e234134a6da5a552f61d4e905e15c0dc58a3414d74304775de5ba8571128f3548d269b51fdc08d5b646fd9157e0a2bc0c4bec5a9a6048d17d1d6cd941b4d459f1de0c7c1d417f33995d2a8dd670b91f0baaccaaf2802100901711885026a5ec97fbbb801000d0d01185651947c1900e336921d07eb16d0e25a2192829540ad5eeb1c498ba9c6316e16807a55dc2b9a7f3dea2e4a2f485ed1295a96d6ca86851842b3a22f83507f93ac66a1dc341d5d22f592527d8ea5c12db16bbabe24b76b3e1baf825c8dcf147be369fd8c5300fd77d0aa8dce730e4e7442c93c4890023f3a266c9fbc90ebbf72825e798c4c00";
        let gamma_bytes = x"240a80664919b9f7490209cff12bfd81c32c272607dc004661c792082cbe282ef826f56a3822ebd72345f86c7ee9872e23f10d1f2dbf43f8aca5dc2ceb5388a5";
        let delta_bytes = x"f755df8c90edab48ac5adafef6a5a461902217f392e3aa4c34c0462b700c18164f79018778755980d491647de11ecc51fda2cc17171c4b44485ec37ccd23a69b";
        let pvk = groth16::pvk_from_bytes(vk_bytes, alpha_bytes, gamma_bytes, delta_bytes);

        // We give 9 equal inputs which exceeds the limit of 8
        let inputs_bytes = x"3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a3fd7c445c6845a9399d1a7b8394c16373399a037786c169f16219359d3be840a";
        let inputs = groth16::public_proof_inputs_from_bytes(inputs_bytes);

        let proof_bytes = x"dd2ef02e57d6a282df6b7f36c134ab7e55c2e04c5b8cbd7831be18e0e7224623ae8bd6c41637c10cbd02f5e68de6394461f417895ddd264d6f0ddacf68c6cd02feb8881f0efa599139a6faf4223dd8743777c4346cba52322eb466af96f2be9f813af1450f84d6f8029804f60cac1add70ad1a3d4226404f84f4022dc18caa0f";
        let proof = groth16::proof_points_from_bytes(proof_bytes);

        groth16::verify_groth16_proof(&curve, &pvk, &inputs, &proof);
    }
}