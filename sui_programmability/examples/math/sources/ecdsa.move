// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A basic ECDSA utility contract to do the following:
// 1) Hash a piece of data using Keccak256, output an object with hashed data.
// 2) Recover a Secp256k1 signature to its public key, output an object with the public key.
// 3) Verify a Secp256k1 signature, produce an event for whether it is verified.
module math::ecdsa_k1 {
    use sui::ecdsa_k1;
    use sui::event;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;
    use std::vector;
    /// Event on whether the signature is verified
    struct VerifiedEvent has copy, drop {
        is_verified: bool,
    }

    /// Object that holds the output data
    struct Output has key, store {
        id: UID,
        value: vector<u8>
    }
    
    /// Hash the data using Keccak256, output an object with the hash to recipient.
    public entry fun keccak256(data: vector<u8>, recipient: address, ctx: &mut TxContext) {
        let hashed = Output {
            id: object::new(ctx),
            value: sui::hash::keccak256(&data),
        };
        // Transfer an output data object holding the hashed data to the recipient.
        transfer::public_transfer(hashed, recipient)
    }

    /// Recover the public key using the signature and message, assuming the signature was produced over the 
    /// Keccak256 hash of the message. Output an object with the recovered pubkey to recipient.
    public entry fun ecrecover(signature: vector<u8>, msg: vector<u8>, recipient: address, ctx: &mut TxContext) {
        let pubkey = Output {
            id: object::new(ctx),
            value: ecdsa_k1::secp256k1_ecrecover(&signature, &msg, 0),
        };
        // Transfer an output data object holding the pubkey to the recipient.
        transfer::public_transfer(pubkey, recipient)
    }

    /// Recover the Ethereum address using the signature and message, assuming 
    /// the signature was produced over the Keccak256 hash of the message. 
    /// Output an object with the recovered address to recipient.
    public entry fun ecrecover_to_eth_address(signature: vector<u8>, msg: vector<u8>, recipient: address, ctx: &mut TxContext) {
        // Normalize the last byte of the signature to be 0 or 1.
        let v = vector::borrow_mut(&mut signature, 64);
        if (*v == 27) {
            *v = 0;
        } else if (*v == 28) {
            *v = 1;
        } else if (*v > 35) {
            *v = (*v - 1) % 2;
        };

        // Ethereum signature is produced with Keccak256 hash of the message, so the last param is 0. 
        let pubkey = ecdsa_k1::secp256k1_ecrecover(&signature, &msg, 0);
        let uncompressed = ecdsa_k1::decompress_pubkey(&pubkey);

        // Take the last 64 bytes of the uncompressed pubkey.
        let uncompressed_64 = vector::empty<u8>();
        let i = 1;
        while (i < 65) {
            let value = vector::borrow(&uncompressed, i);
            vector::push_back(&mut uncompressed_64, *value);
            i = i + 1;
        };

        // Take the last 20 bytes of the hash of the 64-bytes uncompressed pubkey.
        let hashed = sui::hash::keccak256(&uncompressed_64);
        let addr = vector::empty<u8>();
        let i = 12;
        while (i < 32) {
            let value = vector::borrow(&hashed, i);
            vector::push_back(&mut addr, *value);
            i = i + 1;
        };

        let addr_object = Output {
            id: object::new(ctx),
            value: addr,
        };

        // Transfer an output data object holding the address to the recipient.
        transfer::public_transfer(addr_object, recipient)
    }

    /// Verified the secp256k1 signature using public key and message assuming Keccak was using when 
    /// signing. Emit an is_verified event of the verification result.
    public entry fun secp256k1_verify(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>) {
        event::emit(VerifiedEvent {is_verified: ecdsa_k1::secp256k1_verify(&signature, &public_key, &msg, 0)});
    }
}
