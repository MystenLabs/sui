// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Module for storing authenticator state, which is currently just the set of valid JWKs used by
// zklogin.
//
// This module is not currently accessible from user contracts, and is used only to record the JWK
// state to the chain for auditability + restore from snapshot purposes.
module sui::authenticator_state {
    use sui::dynamic_field;
    use std::string::String;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Sender is not @0x0 the system address.
    const ENotSystemAddress: u64 = 0;
    const EWrongInnerVersion: u64 = 1;

    const CurrentVersion: u64 = 1;

    /// Singleton shared object which stores the global authenticator state.
    /// The actual state is stored in a dynamic field of type AuthenticatorStateInner to support
    /// future versions of the authenticator state.
    struct AuthenticatorState has key {
        id: UID,
        version: u64,
    }

    struct AuthenticatorStateInner has store {
        version: u64,

        /// List of currently active JWKs.
        active_jwks: vector<ActiveJwk>,
    }

    #[allow(unused_field)]
    /// Must match the JWK struct in fastcrypto-zkp
    struct JWK has store, drop, copy {
        kty: String,
        e: String,
        n: String,
        alg: String,
    }

    #[allow(unused_field)]
    /// Must match the JwkId struct in fastcrypto-zkp
    struct JwkId has store, drop, copy {
        iss: String,
        kid: String,
    }

    #[allow(unused_field)]
    struct ActiveJwk has store, drop, copy {
        jwk_id: JwkId,
        jwk: JWK,
        epoch: u64,
    }

    #[allow(unused_function)]
    /// Create and share the AuthenticatorState object. This function is call exactly once, when
    /// the authenticator state object is first created.
    fun create(ctx: &TxContext) {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);

        let version = CurrentVersion;

        let inner = AuthenticatorStateInner {
            version,
            active_jwks: vector[],
        };

        let self = AuthenticatorState {
            id: object::authenticator_state(),
            version,
        };

        dynamic_field::add(&mut self.id, version, inner);
        transfer::share_object(self);
    }

    #[allow(unused_function)]
    /// Record a new set of active_jwks. Called when executing the AuthenticatorStateUpdate system
    /// transaction.
    fun update_authenticator_state(
        self: &mut AuthenticatorState,
        active_jwks: vector<ActiveJwk>,
        ctx: &TxContext,
    ) {
        // Validator will make a special system call with sender set as 0x0.
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);

        let version = self.version;

        // replace this with an update function when we add a new version of the inner object.
        assert!(version == CurrentVersion, EWrongInnerVersion);

        let inner: &mut AuthenticatorStateInner = dynamic_field::borrow_mut(&mut self.id, self.version);

        assert!(inner.version == version, EWrongInnerVersion);

        inner.active_jwks = active_jwks;
    }

    #[allow(unused_function)]
    /// Get the current active_jwks. Called when the node starts up in order to load the current
    /// JWK state from the chain.
    fun get_active_jwks(
        self: &AuthenticatorState,
        ctx: &TxContext,
    ): vector<ActiveJwk> {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);

        let version = self.version;
        assert!(version == CurrentVersion, EWrongInnerVersion);

        let inner: &AuthenticatorStateInner = dynamic_field::borrow(&self.id, version);
        assert!(inner.version == version, EWrongInnerVersion);

        inner.active_jwks
    }
}
