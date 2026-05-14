// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::halo2_kzg;

const EUnsupportedKzgVariant: u64 = 1;
const EInvalidDigestLength: u64 = 2;

const KZG_GWC: u8 = 0;
const KZG_SHPLONK: u8 = 1;
const ABI_VERSION: u64 = 1;

public fun abi_version(): u64 { ABI_VERSION }

public fun kzg_gwc(): u8 { KZG_GWC }

public fun kzg_shplonk(): u8 { KZG_SHPLONK }

public fun verify_proof(
    params: vector<u8>,
    params_digest: vector<u8>,
    vk: vector<u8>,
    vk_digest: vector<u8>,
    circuit_info: vector<u8>,
    circuit_info_digest: vector<u8>,
    public_inputs: vector<u8>,
    proof: vector<u8>,
    kzg_variant: u8,
    k_present: bool,
    k: u32,
): bool {
    assert!(params_digest.length() == 32, EInvalidDigestLength);
    assert!(vk_digest.length() == 32, EInvalidDigestLength);
    assert!(circuit_info_digest.length() == 32, EInvalidDigestLength);
    assert!(
        kzg_variant == KZG_GWC || kzg_variant == KZG_SHPLONK,
        EUnsupportedKzgVariant,
    );

    verify_proof_internal(
        params,
        params_digest,
        vk,
        vk_digest,
        circuit_info,
        circuit_info_digest,
        public_inputs,
        proof,
        kzg_variant,
        k_present,
        k,
    )
}

native fun verify_proof_internal(
    params: vector<u8>,
    params_digest: vector<u8>,
    vk: vector<u8>,
    vk_digest: vector<u8>,
    circuit_info: vector<u8>,
    circuit_info_digest: vector<u8>,
    public_inputs: vector<u8>,
    proof: vector<u8>,
    kzg_variant: u8,
    k_present: bool,
    k: u32,
): bool;
