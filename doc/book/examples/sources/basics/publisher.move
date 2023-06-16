// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A simple package that defines an OTW and claims a `Publisher`
/// object for the sender.
module examples::owner {
    use sui::tx_context::TxContext;
    use sui::package;

    /// OTW is a struct with only `drop` and is named
    /// after the module - but uppercased. See "One Time
    /// Witness" page for more details.
    struct OWNER has drop {}

    /// Some other type to use in a dummy check
    struct ThisType {}

    #[allow(unused_function)]
    /// After the module is published, the sender will receive
    /// a `Publisher` object. Which can be used to set Display
    /// or manage the transfer policies in the `Kiosk` system.
    fun init(otw: OWNER, ctx: &mut TxContext) {
        package::claim_and_keep(otw, ctx)
    }
}

/// A module that utilizes the `Publisher` object to give a token
/// of appreciation and a `TypeOwnerCap` for the owned type.
module examples::type_owner {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::package::{Self, Publisher};

    /// Trying to claim ownership of a type with a wrong `Publisher`.
    const ENotOwner: u64 = 0;

    /// A capability granted to those who want an "objective"
    /// confirmation of their ownership :)
    struct TypeOwnerCap<phantom T> has key, store {
        id: UID
    }

    /// Uses the `Publisher` object to check if the caller owns the type `T`.
    public fun prove_ownership<T>(
        publisher: &Publisher, ctx: &mut TxContext
    ): TypeOwnerCap<T> {
        assert!(package::from_package<T>(publisher), ENotOwner);
        TypeOwnerCap<T> { id: object::new(ctx) }
    }
}
