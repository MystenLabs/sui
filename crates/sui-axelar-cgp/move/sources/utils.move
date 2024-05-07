// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module axelar::utils {
    use std::vector;

    use sui::bcs;
    use sui::hash;

    const EInvalidSignatureLength: u64 = 0;

    /// Prefix for Sui Messages.
    const PREFIX: vector<u8> = b"\x19Sui Signed Message:\n";

    /// Normalize last byte of the signature. Have it 1 or 0.
    /// See https://tech.mystenlabs.com/cryptography-in-sui-cross-chain-signature-verification/
    public fun normalize_signature(signature: &mut vector<u8>) {
        // Compute v = 0 or 1.
        assert!(vector::length(signature) == 65, EInvalidSignatureLength);
        let v = vector::borrow_mut(signature, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };
    }

    /// Add a prefix to the bytes.
    public fun to_sui_signed(bytes: vector<u8>): vector<u8> {
        let mut res = vector[];
        vector::append(&mut res, PREFIX);
        vector::append(&mut res, bytes);
        res
    }

    /// Compute operators hash from the list of `operators` (public keys).
    /// This hash is used in `Axelar.epoch_for_hash`.
    public fun operators_hash(operators: &vector<vector<u8>>, weights: &vector<u128>, threshold: u128): vector<u8> {
        let mut data = bcs::to_bytes(operators);
        vector::append(&mut data, bcs::to_bytes(weights));
        vector::append(&mut data, bcs::to_bytes(&threshold));
        hash::keccak256(&data)
    }
}
