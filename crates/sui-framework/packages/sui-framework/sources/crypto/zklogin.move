// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::zklogin {
    use std::string::String;
    use sui::object;
    use sui::object::UID;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    /// Error if any of the inputs are longer than the allowed upper bounds.
    const EInvalidInput: u64 = 0;

    /// Error if the proof consisting of the inputs provided to the verification function is invalid.
    const EInvalidProof: u64 = 1;

    /// Posession of a VerifiedID proves that the user's address was created using zklogin and the
    /// given parameters
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
    public fun get_address_from_verified_id(verified_id: &VerifiedID): address {
        verified_id.address
    }

    /// Returns the name of the key claim associated with the given VerifiedID
    public fun get_kc_name_from_verified_id(verified_id: &VerifiedID): String {
        verified_id.kc_name
    }

    /// Returns the value of the key claim associated with the given VerifiedID
    public fun get_kc_value_from_verified_id(verified_id: &VerifiedID): String {
        verified_id.kc_value
    }

    /// Returns the issuer associated with the given VerifiedID
    public fun get_iss_from_verified_id(verified_id: &VerifiedID): String {
        verified_id.iss
    }

    /// Returns the audience (wallet) associated with the given VerifiedID
    public fun get_aud_from_verified_id(verified_id: &VerifiedID): String {
        verified_id.aud
    }

    /// Verify that the caller's address was created using zklogin and the given parameters. If so,
    /// a VerifiedID cantaining the caller's id (claim name and value, issuer and wallet id) is
    /// returned. Otherwise, abort.
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
    native fun check_zklogin_id_internal(
        address: address,
        name: &vector<u8>,
        value: &vector<u8>,
        iss: &vector<u8>,
        aud: &vector<u8>,
        pin_hash: u256
    ): bool;

    /// Posession of a VerifiedIssuer proves that the user's address was created using zklogin and
    /// with the given issuer (identity provider).
    struct VerifiedIssuer has key {

        /// The ID of this VerifiedIssuer
        id: UID,

        /// The address this VerifiedID is associated with
        address: address,

        /// The issuer
        iss: String,
    }

    /// Returns the address associated with the given VerifiedIssuer
    public fun get_address_from_verified_issuer(verified_issuer: &VerifiedIssuer): address {
        verified_issuer.address
    }

    /// Returns the issuer associated with the given VerifiedIssuer
    public fun get_iss_from_verified_issuer(verified_issuer: &VerifiedIssuer): String {
        verified_issuer.iss
    }

    /// Verify that the caller's address was created using zklogin with the given issuer. If so,
    /// a VerifiedIssuer object cantaining the issuers id returned. Otherwise, None is returned.
    public fun verify_zklogin_iss(
        address_seed: u256,
        iss: String,
        ctx: &mut TxContext,
    ): VerifiedIssuer {
        assert!(check_zklogin_iss(tx_context::sender(ctx), address_seed, &iss), EInvalidProof);
        VerifiedIssuer {id: object::new(ctx), address: tx_context::sender(ctx), iss}
    }

    /// Returns true if `address` was created using zklogin with the given issuer and address seed.
    public fun check_zklogin_iss(
        address: address,
        address_seed: u256,
        iss: &String,
    ): bool {
        check_zklogin_iss_internal(address, address_seed, std::string::bytes(iss))
    }

    /// Returns true if `address` was created using zklogin with the given issuer and address seed.
    native fun check_zklogin_iss_internal(
        address: address,
        address_seed: u256,
        iss: &vector<u8>,
    ): bool;
}
