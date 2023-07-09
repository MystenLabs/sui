// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
///
module kiosk::owned_kiosk {
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::object::{Self, ID, UID};
    use sui::tx_context::{sender, TxContext};
    use sui::dynamic_field as df;

    const EIncorrectCapObject: u64 = 0;
    const EIncorrectOwnedObject: u64 = 1;

    /// A key-only wrapper for the KioskOwnerCap. Makes sure that the Kiosk can
    /// not be traded altogether with its contents.
    struct OwnedKiosk has key {
        id: UID,
        cap: Option<KioskOwnerCap>
    }

    /// The hot potato making sure the KioskOwnerCap is returned after borrowing.
    struct Borrow { cap_id: ID, owned_id: ID }

    /// The dynamic field to mark the Kiosk as owned (to allow guaranteed owner
    /// checks through the Kiosk).
    struct OwnerMarker has copy, store, drop {}

    /// Wrap the KioskOwnerCap making the Kiosk "owned" and non-transferable.
    public fun new(kiosk: &mut Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext) {
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

        // wrap the Cap in the OwnedKiosk
        transfer::transfer(OwnedKiosk {
            id: object::new(ctx),
            cap: option::some(cap)
        }, sender(ctx));
    }

    /// Borrow the KioskOwnerCap from the OwnedKiosk object; Borrow hot-potato
    /// makes sure that the Cap is returned via `return_cap` call.
    public fun borrow_cap(self: &mut OwnedKiosk): (KioskOwnerCap, Borrow) {
        let cap = option::extract(&mut self.cap);
        let id = object::id(&cap);

        (cap, Borrow {
            owned_id: object::id(self),
            cap_id: id
        })
    }

    /// Return the Cap to the OwnedKiosk object.
    public fun return_cap(self: &mut OwnedKiosk, cap: KioskOwnerCap, borrow: Borrow) {
        let Borrow { owned_id, cap_id } = borrow;
        assert!(object::id(self) == owned_id, EIncorrectOwnedObject);
        assert!(object::id(&cap) == cap_id, EIncorrectCapObject);

        option::fill(&mut self.cap, cap)
    }

    /// Check if the Kiosk is "owned".
    public fun is_owned(kiosk: &Kiosk): bool {
        df::exists_(kiosk::uid(kiosk), OwnerMarker {})
    }
}
