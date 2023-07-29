// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module provides a wrapper for the KioskOwnerCap that makes the Kiosk
/// non-transferable and "owned".
///
module kiosk::personal_kiosk {
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{sender, TxContext};
    use sui::dynamic_field as df;

    /// Trying to return the Cap / Borrow to a wrong PersonalKioskCap object.
    const EIncorrectCapObject: u64 = 0;
    /// Trying to return the Cap / Borrow to a wrong PersonalKioskCap object.
    const EIncorrectOwnedObject: u64 = 1;
    /// Trying to get the owner of a non-personal Kiosk.
    const EKioskNotOwned: u64 = 2;
    /// Trying to make a someone else's Kiosk "personal".
    const EWrongKiosk: u64 = 3;

    /// A key-only wrapper for the KioskOwnerCap. Makes sure that the Kiosk can
    /// not be traded altogether with its contents.
    struct PersonalKioskCap has key {
        id: UID,
        cap: Option<KioskOwnerCap>
    }

    /// The hot potato making sure the KioskOwnerCap is returned after borrowing.
    struct Borrow { cap_id: ID, owned_id: ID }

    /// The dynamic field to mark the Kiosk as owned (to allow guaranteed owner
    /// checks through the Kiosk).
    struct OwnerMarker has copy, store, drop {}

    /// The default setup for the PersonalKioskCap.
    entry fun default(kiosk: &mut Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext) {
        transfer_to_sender(new(kiosk, cap, ctx), ctx);
    }

    /// Wrap the KioskOwnerCap making the Kiosk "owned" and non-transferable.
    /// The `PersonalKioskCap` is returned to allow chaining within a PTB, but
    /// the value must be consumed by the `transfer_to_sender` call in any case.
    public fun new(
        kiosk: &mut Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext
    ): PersonalKioskCap {
        assert!(kiosk::has_access(kiosk, &cap), EWrongKiosk);

        let owner = sender(ctx);

        // set the owner property of the Kiosk
        kiosk::set_owner(kiosk, &cap, ctx);

        // add the owner marker to the Kiosk; uses `_as_owner` to always pass,
        // even if Kiosk "allow_extensions" is set to false
        df::add(
            kiosk::uid_mut_as_owner(kiosk, &cap),
            OwnerMarker {},
            owner
        );

        // wrap the Cap in the `PersonalKioskCap`
        PersonalKioskCap {
            id: object::new(ctx),
            cap: option::some(cap)
        }
    }

    /// Borrow the `KioskOwnerCap` from the `PersonalKioskCap` object.
    public fun borrow(self: &PersonalKioskCap): &KioskOwnerCap {
        option::borrow(&self.cap)
    }

    /// Mutably borrow the `KioskOwnerCap` from the `PersonalKioskCap` object.
    public fun borrow_mut(self: &mut PersonalKioskCap): &mut KioskOwnerCap {
        option::borrow_mut(&mut self.cap)
    }

    /// Borrow the `KioskOwnerCap` from the `PersonalKioskCap` object; `Borrow`
    /// hot-potato makes sure that the Cap is returned via `return_val` call.
    public fun borrow_val(
        self: &mut PersonalKioskCap
    ): (KioskOwnerCap, Borrow) {
        let cap = option::extract(&mut self.cap);
        let id = object::id(&cap);

        (cap, Borrow {
            owned_id: object::id(self),
            cap_id: id
        })
    }

    /// Return the Cap to the PersonalKioskCap object.
    public fun return_val(
        self: &mut PersonalKioskCap, cap: KioskOwnerCap, borrow: Borrow
    ) {
        let Borrow { owned_id, cap_id } = borrow;
        assert!(object::id(self) == owned_id, EIncorrectOwnedObject);
        assert!(object::id(&cap) == cap_id, EIncorrectCapObject);

        option::fill(&mut self.cap, cap)
    }

    /// Check if the Kiosk is "personal".
    public fun is_personal(kiosk: &Kiosk): bool {
        df::exists_(kiosk::uid(kiosk), OwnerMarker {})
    }

    /// Get the owner of the Kiosk if the Kiosk is "personal". Aborts otherwise.
    public fun owner(kiosk: &Kiosk): address {
        assert!(is_personal(kiosk), EKioskNotOwned);
        *df::borrow(kiosk::uid(kiosk), OwnerMarker {})
    }

    /// Transfer the `PersonalKioskCap` to the transaction sender.
    public fun transfer_to_sender(self: PersonalKioskCap, ctx: &mut TxContext) {
        transfer::transfer(self, sender(ctx));
    }
}
