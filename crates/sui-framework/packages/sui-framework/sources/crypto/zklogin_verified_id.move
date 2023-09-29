// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::zklogin_verified_id {
    use std::string::String;
    use sui::object;
    use sui::object::UID;
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
        address: address,
        /// The name of the key claim
        kc_name: String,
        /// The value of the key claim
        kc_value: String,
        /// The issuer
        iss: String,

        /// The audience (wallet)
        aud: String,
    }

    /// Returns the address associated with the given VerifiedID
    public fun address(verified_id: &VerifiedID): address {
        verified_id.address
    }

    /// Returns the name of the key claim associated with the given VerifiedID
    public fun kc_name(verified_id: &VerifiedID): &String {
        &verified_id.kc_name
    }

    /// Returns the value of the key claim associated with the given VerifiedID
    public fun kc_value(verified_id: &VerifiedID): String {
        &verified_id.kc_value
    }

    /// Returns the issuer associated with the given VerifiedID
    public fun iss(verified_id: &VerifiedID): &String {
        &verified_id.iss
    }

    /// Returns the audience (wallet) associated with the given VerifiedID
    public fun aud(verified_id: &VerifiedID): &String {
        &verified_id.aud
    }

    /// Verify that the caller's address was created using zklogin and the given parametersand returns a `VerifiedID`
    /// with the caller's id (claim name and value, issuer and wallet id).
    ///
    /// Aborts with `EInvalidInput` if any of the inputs are longer than the allowed upper bounds: `kc_name` must be at
    /// most 32 characters, `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    ///
    /// Aborts with `EInvalidProof` if the verification fails.
    public fun verify_zklogin_id(
        kc_name: String,
        kc_value: String,
        iss: String,
        aud: String,
        pin_hash: u256,
        ctx: &mut TxContext,
    ): VerifiedID {
        assert!(check_zklogin_id(tx_context::sender(ctx), &kc_name, &kc_value, &iss, &aud, pin_hash), EInvalidProof);
        VerifiedID { id: object::new(ctx), address: tx_context::sender(ctx), kc_name, kc_value, iss, aud}
    }

    /// Returns true if `address` was created using zklogin and the given parameters.
    ///
    /// Aborts with `EInvalidInput` if any of the inputs are longer than the allowed upper bounds: `kc_name` must be at
    /// most 32 characters, `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    public fun check_zklogin_id(
        address: address,
        kc_name: &String,
        kc_value: &String,
        iss: &String,
        aud: &String,
        pin_hash: u256
    ): bool {
        check_zklogin_id_internal(address,
            std::string::bytes(kc_name),
            std::string::bytes(kc_value),
            std::string::bytes(iss),
            std::string::bytes(aud),
            pin_hash)
    }

    /// Returns true if `address` was created using zklogin and the given parameters.
    ///
    /// Aborts with `EInvalidInput` if any of `kc_name`, `kc_value`, `iss` and `aud` is not a properly encoded UTF-8
    /// string or if the inputs are longer than the allowed upper bounds: `kc_name` must be at most 32 characters,
    /// `kc_value` must be at most 115 characters and `aud` must be at most 145 characters.
    native fun check_zklogin_id_internal(
        address: address,
        kc_name: &vector<u8>,
        kc_value: &vector<u8>,
        iss: &vector<u8>,
        aud: &vector<u8>,
        pin_hash: u256
    ): bool;
}
