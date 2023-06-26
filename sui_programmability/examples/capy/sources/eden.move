// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple permissionless application that stores two capys and
/// breeds them on request.
///
/// Enables "get free capy" functionality. A single account can request
/// up to 2 capys; then it aborts.
module capy::eden {
    use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::dynamic_field as dfield;

    use capy::capy::{Self, Capy, CapyRegistry};

    /// For when someone tries to breed more than 2 `Capy`s.
    const EMaxBred: u64 = 0;

    /// A shared object containing 2 Capys for free breeding.
    struct Eden has key {
        id: UID,
        capy_one: Option<Capy>,
        capy_two: Option<Capy>,
    }

    #[allow(unused_function)]
    fun init(ctx: &mut TxContext) {
        sui::transfer::share_object(Eden {
            id: object::new(ctx),
            capy_one: option::none(),
            capy_two: option::none()
        })
    }

    /// Admin-only action to set 2 capys for breeding in the `Eden` object.
    entry fun set(eden: &mut Eden, capy_one: Capy, capy_two: Capy) {
        option::fill(&mut eden.capy_one, capy_one);
        option::fill(&mut eden.capy_two, capy_two)
    }

    #[allow(unused_function)]
    /// Breed a "free" Capy using capys set in the `Eden` object. Can only be performed
    /// twice. Aborts when trying to breed more than 2 times.
    fun get_capy(eden: &mut Eden, reg: &mut CapyRegistry, ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);
        let total = if (dfield::exists_with_type<address, u64>(&eden.id, sender)) {
            let total = dfield::remove<address, u64>(&mut eden.id, sender);
            assert!(total != 2, EMaxBred);
            total
        } else {
            0
        };

        dfield::add<address, u64>(&mut eden.id, sender, total + 1);
        capy::breed_and_keep(
            reg,
            option::borrow_mut(&mut eden.capy_one),
            option::borrow_mut(&mut eden.capy_two),
            ctx
        )
    }
}
