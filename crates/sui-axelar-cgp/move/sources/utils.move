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
        assert!(vector::length<u8>(signature) == 65, EInvalidSignatureLength);
        let v = vector::borrow_mut(signature, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };
    }

    public fun abi_encode_start(len: u64): vector<u8> {
        let v = vector::empty<u8>();
        let i = 0;
        while(i < 32 * len) {
            vector::push_back<u8>(&mut v, 0);
            i = i + 1;
        };
        v
    }

    public fun abi_encode_fixed(v: &mut vector<u8>, pos: u64, var: u256) {
        let i = 0;
        while( i < 32 ) {
            let val = vector::borrow_mut<u8>(v, i + 32 * pos);
            let exp = ((31 - i) * 8 as u8);
            *val = (var >> exp & 255 as u8);
            i = i + 1;
        };
    }

    public fun abi_encode_variable(v: &mut vector<u8>, pos: u64, var: vector<u8>) {
        let length = vector::length<u8>(v);
        abi_encode_fixed(v, pos, (length as u256));
        let i: u64 = 0;
        while( i < 32 - 8 ){
            vector::push_back<u8>(v, 0);
            i = i + 1;
        };
        i = 0;
        length = vector::length<u8>(&var);
        while( i < 8 ) {
            vector::push_back<u8>(v, ((length >> ((7 - i) * 8 as u8)) & 255 as u8));
            i = i + 1;
        };
        vector::append<u8>(v, var);
        i = 0;
        while( i < 31 - (length - 1) % 32 ) {
            vector::push_back<u8>(v, 0);
            i = i + 1;
        };
    }

    public fun abi_decode_fixed(v: vector<u8>, pos: u64) : u256 {
        let var : u256 = 0;
        let i = 0;
        while(i < 32) {
            var = var << 8;
            var = var | (*vector::borrow<u8>(&v, i + 32 * pos) as u256);
            i = i + 1;
        };
        var
    }

    public fun abi_decode_variable(v: vector<u8>, pos: u64): vector<u8> {
        let start = (abi_decode_fixed(v, pos) as u64);
        let len = (abi_decode_fixed(v, start / 32) as u64);
        let var = vector::empty<u8>();
        let i = 0;
        while(i < len) {
            vector::push_back<u8>(&mut var, *vector::borrow<u8>(&v, i + start + 32));
            i = i + 1;
        };
        var
    }

    /// Add a prefix to the bytes.
    public fun to_sui_signed(bytes: vector<u8>): vector<u8> {
        let res = vector[];
        vector::append<u8>(&mut res, PREFIX);
        vector::append<u8>(&mut res, bytes);
        res
    }

    /// Compute operators hash from the list of `operators` (public keys).
    /// This hash is used in `Axelar.epoch_for_hash`.
    public fun operators_hash(operators: &vector<vector<u8>>, weights: &vector<u128>, threshold: u128): vector<u8> {
        let data = bcs::to_bytes(operators);
        vector::append<u8>(&mut data, bcs::to_bytes(weights));
        vector::append<u8>(&mut data, bcs::to_bytes(&threshold));
        hash::keccak256(&data)
    }


    
    #[test_only]
    const VAR1: vector<u8> = x"037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff599028";
    #[test_only]
    const VAR2: vector<u8> = x"000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000002370000000000000000000000000000000000000000000000000000000000000021037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902800000000000000000000000000000000000000000000000000000000000000";

    #[test_only]
    const FIX1: u256 = 235893452345934825970329407589;
    #[test_only]
    const FIX2: u256 = 2343532458234812893949583;

    #[test_only]
    const RESULT: vector<u8> = x"0000000000000000000000000000000000000002fa367d8b6dacc5919d8f5065000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000001f043262d1572d472ee8f00000000000000000000000000000000000000000000000000000000000000e00000000000000000000000000000000000000000000000000000000000000021037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff5990280000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000007b000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000002370000000000000000000000000000000000000000000000000000000000000021037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902800000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fun abi_encode_test() {
        let v = abi_encode_start(4);
        abi_encode_fixed(&mut v, 0, FIX1);
        abi_encode_fixed(&mut v, 2, FIX2);
        abi_encode_variable(&mut v, 1, VAR1);
        abi_encode_variable(&mut v, 3, VAR2);

        assert!(&v == &RESULT, 1);

        let fix1 = abi_decode_fixed(v, 0);
        let var1 = abi_decode_variable(v, 1);
        let fix2 = abi_decode_fixed(v, 2);
        let var2 = abi_decode_variable(v, 3);

        
        assert!(&fix1 == &FIX1, 1);
        assert!(&var1 == &VAR1, 1);
        assert!(&fix2 == &FIX2, 1);
        assert!(&var2 == &VAR2, 1);
    }
}
