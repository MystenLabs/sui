// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::ed25519 {
    friend sui::validator;
    /// @param signature: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param public_key: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param msg: The message that we test the signature against.
    ///
    /// If the signature is a valid Ed25519 signature of the message and public key, return true.
    /// Otherwise, return false.
    public native fun ed25519_verify(signature: &vector<u8>, public_key: &vector<u8>, msg: &vector<u8>): bool;

    /// @param signature: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param public_key: 32-byte signature that is a point on the Ed25519 elliptic curve.
    /// @param msg: The message that we test the signature against.
    /// @param domain: The domain that the signature is tested again. We essentially prepend this to the message.
    ///
    /// If the signature is a valid Ed25519 signature of the message and public key, return true.
    /// Otherwise, return false.
    public(friend) fun ed25519_verify_with_domain(signature: &vector<u8>, public_key: &vector<u8>, msg: vector<u8>, domain: vector<u8>): bool {
        std::vector::append(&mut domain, msg);
        ed25519_verify(signature, public_key, &domain)
    }

}
