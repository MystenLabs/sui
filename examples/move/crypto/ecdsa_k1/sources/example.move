// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A basic ECDSA utility contract to do the following:
///
/// 1) Hash a piece of data using Keccak256, output an object with hashed data.
/// 2) Recover a Secp256k1 signature to its public key, output an
///    object with the public key.
/// 3) Verify a Secp256k1 signature, produce an event for whether it is verified.
module ecdsa_k1::example;

use sui::{ecdsa_k1, event};

// === Object Types ===

/// Object that holds the output data
public struct Output has key, store {
    id: UID,
    value: vector<u8>,
}

// == Event Types ===

/// Event on whether the signature is verified
public struct VerifiedEvent has copy, drop {
    is_verified: bool,
}

// === Public Functions ===

/// Hash the data using Keccak256, output an object with the hash to recipient.
public fun keccak256(data: vector<u8>, recipient: address, ctx: &mut TxContext) {
    let hashed = Output {
        id: object::new(ctx),
        value: sui::hash::keccak256(&data),
    };
    // Transfer an output data object holding the hashed data to the recipient.
    transfer::public_transfer(hashed, recipient)
}

/// Recover the public key using the signature and message, assuming the signature was produced
/// over the Keccak256 hash of the message. Output an object with the recovered pubkey to
/// recipient.
public fun ecrecover(
    signature: vector<u8>,
    msg: vector<u8>,
    recipient: address,
    ctx: &mut TxContext,
) {
    let pubkey = Output {
        id: object::new(ctx),
        value: ecdsa_k1::secp256k1_ecrecover(&signature, &msg, 0),
    };
    // Transfer an output data object holding the pubkey to the recipient.
    transfer::public_transfer(pubkey, recipient)
}

/// Recover the Ethereum address using the signature and message, assuming the signature was
/// produced over the Keccak256 hash of the message. Output an object with the recovered address
/// to recipient.
public fun ecrecover_to_eth_address(
    mut signature: vector<u8>,
    msg: vector<u8>,
    recipient: address,
    ctx: &mut TxContext,
) {
    // Normalize the last byte of the signature to be 0 or 1.
    let v = &mut signature[64];
    if (*v == 27) {
        *v = 0;
    } else if (*v == 28) {
        *v = 1;
    } else if (*v > 35) {
        *v = (*v - 1) % 2;
    };

    // Ethereum signature is produced with Keccak256 hash of the message, so the last param is
    // 0.
    let pubkey = ecdsa_k1::secp256k1_ecrecover(&signature, &msg, 0);
    let uncompressed = ecdsa_k1::decompress_pubkey(&pubkey);

    // Take the last 64 bytes of the uncompressed pubkey.
    let mut uncompressed_64 = vector[];
    let mut i = 1;
    while (i < 65) {
        uncompressed_64.push_back(uncompressed[i]);
        i = i + 1;
    };

    // Take the last 20 bytes of the hash of the 64-bytes uncompressed pubkey.
    let hashed = sui::hash::keccak256(&uncompressed_64);
    let mut addr = vector[];
    let mut i = 12;
    while (i < 32) {
        addr.push_back(hashed[i]);
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
public fun secp256k1_verify(signature: vector<u8>, public_key: vector<u8>, msg: vector<u8>) {
    event::emit(VerifiedEvent {
        is_verified: ecdsa_k1::secp256k1_verify(&signature, &public_key, &msg, 0),
    });
}
