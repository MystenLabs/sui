// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::zklogin {
    use std::option;
    use std::option::Option;
    use std::string::String;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    /// Error if any of the inputs are longer than the allowed upper bounds
    const EInvalidInput: u64 = 0;

    /// Posession of a VerifiedID proves that the user's address was created using zklogin and the
    /// given parameters
    struct VerifiedID {
        /// The name of the claim
        kc_name: String,

        /// The value of the claim
        kc_value: String,

        /// The issuer
        iss: String,

        /// The audience (wallet)
        aud: String,
    }

    /// Verify that the caller's address was created using zklogin and the given parameters. If so,
    /// a VerifiedID cantaining the caller's id (claim name and value, issuer and wallet id) is
    /// returned. Otherwise, None is returned.
    public fun verify_zklogin_id(
        ctx: &TxContext,
        kc_name: String,
        kc_value: String,
        iss: String,
        aud: String,
        pin_hash: u256
    ): Option<VerifiedID> {
        if (check_zklogin_id(tx_context::sender(ctx), &kc_name, &kc_value, &iss, &aud, pin_hash)) {
            option::some(VerifiedID {kc_name, kc_value, iss, aud})
        } else {
            option::none()
        }
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
    struct VerifiedIssuer {
        /// The issuer
        iss: String,
    }

    /// Verify that the caller's address was created using zklogin with the given issuer. If so,
    /// a VerifiedIssuer object cantaining the issuers id returned. Otherwise, None is returned.
    public fun verify_zklogin_iss(
        ctx: &mut TxContext,
        address_seed: u256,
        iss: String,
    ): Option<VerifiedIssuer> {
        if (check_zklogin_iss(tx_context::sender(ctx), address_seed, &iss)) {
            option::some(VerifiedIssuer {iss})
        } else {
            option::none()
        }
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
