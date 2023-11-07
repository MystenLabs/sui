// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_const)]
module sui::zklogin_verified_id {
    use std::string::String;
    use sui::object;
    use sui::object::UID;
    use sui::transfer;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    /// Error if any of the inputs are longer than the allowed upper bounds.
    const EInvalidInput: u64 = 0;

    /// Error if the proof consisting of the inputs provided to the verification function is invalid.
    const EInvalidProof: u64 = 1;

    /// Posession of a VerifiedID proves that the user's address was created using zklogin and the given parameters.
    struct VerifiedID has key {
        /// The ID of this VerifiedID
        id: UID,
        /// The address this VerifiedID is associated with
        owner: address,
        /// The name of the key claim
        key_claim_name: String,
        /// The value of the key claim
        key_claim_value: String,
        /// The issuer
        issuer: String,
        /// The audience (wallet)
        audience: String,
    }

    /// Returns the address associated with the given VerifiedID
    public fun owner(verified_id: &VerifiedID): address {
        verified_id.owner
    }

    /// Returns the name of the key claim associated with the given VerifiedID
    public fun key_claim_name(verified_id: &VerifiedID): &String {
        &verified_id.key_claim_name
    }

    /// Returns the value of the key claim associated with the given VerifiedID
    public fun key_claim_value(verified_id: &VerifiedID): &String {
        &verified_id.key_claim_value
    }

    /// Returns the issuer associated with the given VerifiedID
    public fun issuer(verified_id: &VerifiedID): &String {
        &verified_id.issuer
    }

    /// Returns the audience (wallet) associated with the given VerifiedID
    public fun audience(verified_id: &VerifiedID): &String {
        &verified_id.audience
    }

    /// Verify that the caller's address was created using zklogin and the given parametersand returns a `VerifiedID`
    /// with the caller's id (claim name and value, issuer and wallet id).
    ///
    /// Aborts with `EInvalidInput` if any of the inputs are longer than the allowed upper bounds: `kc_name` must be at
    /// most 32 characters, `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    ///
    /// Aborts with `EInvalidProof` if the verification fails.
    public fun verify_zklogin_id(
        key_claim_name: String,
        key_claim_value: String,
        issuer: String,
        audience: String,
        pin_hash: u256,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        assert!(check_zklogin_id(sender, &key_claim_name, &key_claim_value, &issuer, &audience, pin_hash), EInvalidProof);
        transfer::transfer(
            VerifiedID {
                id: object::new(ctx),
                owner: sender,
                key_claim_name,
                key_claim_value,
                issuer,
                audience
            },
            sender
        );
    }

    /// Returns true if `address` was created using zklogin and the given parameters.
    ///
    /// Aborts with `EInvalidInput` if any of the inputs are longer than the allowed upper bounds: `kc_name` must be at
    /// most 32 characters, `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    public fun check_zklogin_id(
        address: address,
        key_claim_name: &String,
        key_claim_value: &String,
        issuer: &String,
        audience: &String,
        pin_hash: u256
    ): bool {
        check_zklogin_id_internal(
            address,
            std::string::bytes(key_claim_name),
            std::string::bytes(key_claim_value),
            std::string::bytes(issuer),
            std::string::bytes(audience),
            pin_hash
        )
    }

    /// Returns true if `address` was created using zklogin and the given parameters.
    ///
    /// Aborts with `EInvalidInput` if any of `kc_name`, `kc_value`, `iss` and `aud` is not a properly encoded UTF-8
    /// string or if the inputs are longer than the allowed upper bounds: `kc_name` must be at most 32 characters,
    /// `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    native fun check_zklogin_id_internal(
        address: address,
        key_claim_name: &vector<u8>,
        key_claim_value: &vector<u8>,
        issuer: &vector<u8>,
        audience: &vector<u8>,
        pin_hash: u256
    ): bool;
}
