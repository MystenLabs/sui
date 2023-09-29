// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::zklogin_verified_iss {
    use std::string::String;
    use sui::object;
    use sui::object::UID;
    use sui::tx_context::TxContext;
    use sui::tx_context;

    /// Error if the proof consisting of the inputs provided to the verification function is invalid.
    const EInvalidInput: u64 = 0;

    /// Error if the proof consisting of the inputs provided to the verification function is invalid.
    const EInvalidProof: u64 = 1;

    /// Posession of a VerifiedIssuer proves that the user's address was created using zklogin and with the given issuer
    /// (identity provider).
    struct VerifiedIssuer has key {
        /// The ID of this VerifiedIssuer
        id: UID,
        /// The address this VerifiedID is associated with
        address: address,
        /// The issuer
        iss: String,
    }

    /// Returns the address associated with the given VerifiedIssuer
    public fun address(verified_issuer: &VerifiedIssuer): address {
        verified_issuer.address
    }

    /// Returns the issuer associated with the given VerifiedIssuer
    public fun iss(verified_issuer: &VerifiedIssuer): &String {
        &verified_issuer.iss
    }

    /// Verify that the caller's address was created using zklogin with the given issuer. If so, a VerifiedIssuer object
    /// with the issuers id returned.
    ///
    /// Aborts with `EInvalidProof` if the verification fails.
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
    ///
    /// Aborts with `EInvalidInput` if the `iss` input is not a valid UTF-8 string.
    native fun check_zklogin_iss_internal(
        address: address,
        address_seed: u256,
        iss: &vector<u8>,
    ): bool;
}
