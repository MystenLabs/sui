// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{NativesCostTable, get_extension};
use fastcrypto::hash::{Blake2b256, HashFunction};
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::{
    execution::{Type, values::Value},
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use std::{collections::VecDeque, panic::AssertUnwindSafe};

pub const E_INPUT_TOO_LARGE: u64 = 0;
pub const E_INVALID_NATIVE_ARGUMENT: u64 = 1;

pub const KZG_GWC: u8 = 0;
pub const KZG_SHPLONK: u8 = 1;

pub const MAX_PARAMS_BYTES: usize = 240 * 1024;
pub const MAX_VK_BYTES: usize = 240 * 1024;
pub const MAX_CIRCUIT_INFO_BYTES: usize = 240 * 1024;
pub const MAX_PROOF_BYTES: usize = 96 * 1024;
pub const MAX_PUBLIC_INPUT_BYTES: usize = 16 * 1024;
pub const MAX_TOTAL_NATIVE_INPUT_BYTES: usize = MAX_PARAMS_BYTES
    + MAX_VK_BYTES
    + MAX_CIRCUIT_INFO_BYTES
    + MAX_PROOF_BYTES
    + MAX_PUBLIC_INPUT_BYTES;

pub fn verify_proof_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 11);

    let invalid_arguments_cost =
        get_extension!(context, NativesCostTable)?.crypto_invalid_arguments_cost;
    context.charge_gas(invalid_arguments_cost)?;
    let cost = context.gas_used();

    let k = pop_arg!(args, u32);
    let k_present = pop_arg!(args, bool);
    let kzg_variant = pop_arg!(args, u8);
    let proof = pop_vector_u8(&mut args)?;
    let public_inputs = pop_vector_u8(&mut args)?;
    let circuit_info_digest = pop_vector_u8(&mut args)?;
    let circuit_info = pop_vector_u8(&mut args)?;
    let vk_digest = pop_vector_u8(&mut args)?;
    let vk = pop_vector_u8(&mut args)?;
    let params_digest = pop_vector_u8(&mut args)?;
    let params = pop_vector_u8(&mut args)?;

    let inputs = NativeVerifyInputs {
        params: &params,
        params_digest: &params_digest,
        vk: &vk,
        vk_digest: &vk_digest,
        circuit_info: &circuit_info,
        circuit_info_digest: &circuit_info_digest,
        public_inputs: &public_inputs,
        proof: &proof,
        kzg_variant,
        k: k_present.then_some(k),
    };

    match verify_halo2_kzg(inputs) {
        VerifyOutcome::Valid => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        VerifyOutcome::Invalid => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        VerifyOutcome::Abort(code) => Ok(NativeResult::err(cost, code)),
    }
}

fn pop_vector_u8(args: &mut VecDeque<Value>) -> PartialVMResult<Vec<u8>> {
    Ok(pop_arg!(args, Vec<u8>))
}

struct NativeVerifyInputs<'a> {
    params: &'a [u8],
    params_digest: &'a [u8],
    vk: &'a [u8],
    vk_digest: &'a [u8],
    circuit_info: &'a [u8],
    circuit_info_digest: &'a [u8],
    public_inputs: &'a [u8],
    proof: &'a [u8],
    kzg_variant: u8,
    k: Option<u32>,
}

enum VerifyOutcome {
    Valid,
    Invalid,
    Abort(u64),
}

fn verify_halo2_kzg(inputs: NativeVerifyInputs<'_>) -> VerifyOutcome {
    if inputs.kzg_variant != KZG_GWC && inputs.kzg_variant != KZG_SHPLONK {
        return VerifyOutcome::Abort(E_INVALID_NATIVE_ARGUMENT);
    }

    if inputs.params_digest.len() != 32
        || inputs.vk_digest.len() != 32
        || inputs.circuit_info_digest.len() != 32
    {
        return VerifyOutcome::Abort(E_INVALID_NATIVE_ARGUMENT);
    }

    let total_input_bytes = inputs.params.len()
        + inputs.vk.len()
        + inputs.circuit_info.len()
        + inputs.public_inputs.len()
        + inputs.proof.len();

    if inputs.params.len() > MAX_PARAMS_BYTES
        || inputs.vk.len() > MAX_VK_BYTES
        || inputs.circuit_info.len() > MAX_CIRCUIT_INFO_BYTES
        || inputs.public_inputs.len() > MAX_PUBLIC_INPUT_BYTES
        || inputs.proof.len() > MAX_PROOF_BYTES
        || total_input_bytes > MAX_TOTAL_NATIVE_INPUT_BYTES
    {
        return VerifyOutcome::Abort(E_INPUT_TOO_LARGE);
    }

    if digest(inputs.params) != inputs.params_digest
        || digest(inputs.vk) != inputs.vk_digest
        || digest(inputs.circuit_info) != inputs.circuit_info_digest
    {
        return VerifyOutcome::Invalid;
    }

    match std::panic::catch_unwind(AssertUnwindSafe(|| {
        halo2_verifier::deserialize_circuit_and_verify(
            inputs.params,
            inputs.vk,
            inputs.circuit_info,
            inputs.public_inputs,
            inputs.proof,
            inputs.kzg_variant,
            inputs.k,
        )
    })) {
        Ok(Ok(())) => VerifyOutcome::Valid,
        Ok(Err(_)) | Err(_) => VerifyOutcome::Invalid,
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Blake2b256::digest(bytes).digest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_inputs<'a>(
        params: &'a [u8],
        params_digest: &'a [u8],
        vk: &'a [u8],
        vk_digest: &'a [u8],
        circuit_info: &'a [u8],
        circuit_info_digest: &'a [u8],
        public_inputs: &'a [u8],
        proof: &'a [u8],
    ) -> NativeVerifyInputs<'a> {
        NativeVerifyInputs {
            params,
            params_digest,
            vk,
            vk_digest,
            circuit_info,
            circuit_info_digest,
            public_inputs,
            proof,
            kzg_variant: KZG_GWC,
            k: None,
        }
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0);
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    #[test]
    fn valid_proof_returns_valid() {
        let params = hex_to_bytes(
            "0400000042f8cfea72663b7832e47dc1fdeab56f2f1d07b729ce0a67a9f95480067c840de1800642e35c0a9fdd474e873eb42c6ceae6d8d8f43c0e035c9fbfb89bb32728303f01830dc2182d175d38ddb47e6cc2c2063b0831432043ac7994d29438082d9c8e35afccd69c2b897efd50d7ed0e62122388f41326dba4bc0d128ae0d14119",
        );
        let vk = hex_to_bytes(
            "040401000000e8d6a310e68ff8ec0c23b3493ad971f39df348d914b69d900ef6bdb1a9380821a0d18c6d3fddc6df07b88dc384fed028707856b47f2ecf104a733b540541091054c1d5267ffcf0bb4847238fc935dff061a38e0c871a88849cf4d6460563c507b42083b1856a6aeb1c85d9942c94d017daa5a4ff5edfc46cf08ba1d025b0e3285adf9bf0d7ac95bb8db1cff4024ada370ee77158c0b3583d79e829e3445280057d2ddbb5c2d6dbc5f2f4d04cde7990d04398ffe4209787b59d4ca8cf3fdfca083bfafcc40b672a8f5e55f6e5b499cfb0f890dde2b36823a527c2d3eee7a1f52d935240d23b082f3a51d7891bc62a0f7af50b2ea077bf40037610850363338004203b2593d81f79267ccc50a960dd60a4ad849d0d22be1f321318a36a595f2a1542a3c0201d2e6111bb8e7e170542056c37d5e8de34633a62d5c5f1f7f3dd452b",
        );
        let circuit_info = hex_to_bytes(
            "0b0c20acc86b4c84170be1ea86dfb0bf5d284c7bee72808a85412c71eeec572b2fbb0b208effc754694da2cb6df0dc36fe4a9bc7e3ec844490da918c007213c66bf786a38001b61dd63efa2807041eec04d2e53c1dcdef061216ff9f65a22d88b152b8d6559f994768be185bbb68e44116cb6d1017bab8dfe91dc3ddb28ed720139f34ea6505d4b098bb2b6a4f0d5ec7d96d3184666aaecda03d0d83cfe4fb06c7edccb9c5a22f720095cbbf541bd781e9d75cfd01d23ff3ca5674e05d85d001abce9688539e010404010000000403000000080100000000000000080100000000000000030000000001000100030a010000000001000000000a010100000001000000000a01020000000100000000010a03000000000100000000010a020000000001000000000405030000000005010000000005010100000005010200000000010c08020007080300030106030200000000",
        );
        let public_inputs = hex_to_bytes(
            "0103200600000000000000000000000000000000000000000000000000000000000000200600000000000000000000000000000000000000000000000000000000000000200600000000000000000000000000000000000000000000000000000000000000",
        );
        let proof = hex_to_bytes(
            "e9445cc7533f61fff8af036209735753b9276900d0b1812e91405ce65da07d20d8994f4c3db10d08f37a602e0f56258c624c6076d800678adfd0ecbad2fc3a2065db9386aa1d60c6c8ccffb869093fada5eb6797ba9488c8fa8b39f39d88ea0d8b987dbc98354df75153951b34d21fef0ec49e419453aa9eabb8397cf70c4e8945f3d19d0b85a80b1c92dd1f67742a65a1407676aae21846619e7683ba3681074fe0f929405e17fdb6b5457fa2796587349001fc43ee6726ef6473a62d772e270c4f6c0720fbd0cc9b142f6b7019cce18ffbb071348b7252d92c15ed49cb34af5fb50e30a6f2ae37b39bbc5e0f08f511623fc2e347f9dbc241b7676af8c2068f4e424a79cdf9e47d3f81213b7ce0754e22a5900bc034d9ec14eb976f2e4fe68f13a964e497d8550450f29c3d207d1319e41d325463add88986caa6226d3f5a071c1a237da662f8bc7044c930ba01e78ebab10c8f3750ce3875ade4c17613f629d469b3c80240d084f8eb7c00d349f1ec2eb923405d065c45e05972117810750c129a65213a381020350769823427aa691c79b77591373c8c9a19ee97717bda1bd2e5fe1e693cea645a56976dac736f1e4729ffc503392660cff16d21c64679144b8ad3b2f7ffd36e26837178cdd403266fe5ef05eee9eccf87032c2be6327007fad06835aceec94fcd5018e0d7001da60be35da0a23d43f702c6a8da7c40433047344f70808328501fce9f1920c3f54187c36e36c4d3d410fe76295a2f0afe07e040ec38b721e1fdb068c0eea9e5ec3b88579674b13a9471f7e8f2d6e82a5e00bf6c1f64443eaac6a3e772d283e6a35839574d39fa183fa8ed0dbb87beb71e2412fe1918f814d4ad50bb2001a6d0afb2c98bbce25b1d516896fa431853965812000be23e6303955802b8c503385837aaf3a84459d99d426d2723d63a20d74c2c91afafcce1aa44c1faaaa5f088da984c557cd591fa0e8bd3ef31ad1c128b1e21e19032fd9c419d73a070165530851f8ddbfab0c6a0ee795428bbbe6ef74cbd2c99ffe15922ee1a3b9ae82e99d5783ad1d3804b5df5ececa5898a58167a426f0ff534a7c85ca4603eb202f5e6ee2993b5bf74ea2fd930687946ff421e0ed8b40f881fb309f422e050f35184f65e4f719c2f0fb8a68ee5de10fc474532d00c2122657efdc65431f313ec8d02ed8f017b9bb14fff3910dfc58f78c5f8f17f32ca1c8fb4abbcb1978e8c5f1d783025eb19fb8580e4f50bb3755136ff1a2c0ac903296bb6136ecf03ba35e23496dfd4d77b728532cb5b41ba46fc489926ddb5951a26b82e74bf0fd684b4f4a1c4728eedece37c52a970f30e5915fb931d804014992ba09392b14eca34c6a8e3915dac6afc2ef43041ec6691f4c7769bf230b9016614391af4f7d9df348ee7a0005c110e0563a65073f5f7383abc98912d8e249e3a13e0242be684a69ebe87cf7da07ede3ba9fd7041db88f3cba0469bd3b582393a9e",
        );

        let outcome = verify_halo2_kzg(test_inputs(
            &params,
            &digest(&params),
            &vk,
            &digest(&vk),
            &circuit_info,
            &digest(&circuit_info),
            &public_inputs,
            &proof,
        ));

        assert!(matches!(outcome, VerifyOutcome::Valid));
    }

    #[test]
    fn wrong_digest_returns_invalid() {
        let params = [1u8, 2, 3];
        let vk = [4u8, 5, 6];
        let circuit_info = [7u8, 8, 9];
        let public_inputs = [];
        let proof = [];
        let wrong_digest = [0u8; 32];

        let outcome = verify_halo2_kzg(test_inputs(
            &params,
            &wrong_digest,
            &vk,
            &digest(&vk),
            &circuit_info,
            &digest(&circuit_info),
            &public_inputs,
            &proof,
        ));

        assert!(matches!(outcome, VerifyOutcome::Invalid));
    }

    #[test]
    fn oversized_proof_aborts() {
        let params = [];
        let vk = [];
        let circuit_info = [];
        let public_inputs = [];
        let proof = vec![0; MAX_PROOF_BYTES + 1];

        let outcome = verify_halo2_kzg(test_inputs(
            &params,
            &digest(&params),
            &vk,
            &digest(&vk),
            &circuit_info,
            &digest(&circuit_info),
            &public_inputs,
            &proof,
        ));

        assert!(matches!(outcome, VerifyOutcome::Abort(E_INPUT_TOO_LARGE)));
    }

    #[test]
    fn malformed_inputs_return_invalid_without_unwinding() {
        let params = [];
        let vk = [];
        let circuit_info = [];
        let public_inputs = [];
        let proof = [];

        let outcome = verify_halo2_kzg(test_inputs(
            &params,
            &digest(&params),
            &vk,
            &digest(&vk),
            &circuit_info,
            &digest(&circuit_info),
            &public_inputs,
            &proof,
        ));

        assert!(matches!(outcome, VerifyOutcome::Invalid));
    }
}
