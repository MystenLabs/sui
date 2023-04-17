// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a module for AirDrops - unifies the interface for airdrops
/// by creating a standard dynamic field to store the airdropped items.
module kiosk::airdrop_ext {
    use sui::dynamic_field as df;
    use sui::object::{Self, ID};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};

    struct ItemDropped<phantom T> has store, copy, drop {
        kiosk_id: ID,
        item_id: ID
    }

    /// AirdropKey is a dynamic field that stores the airdropped items.
    struct AirdropKey<phantom T> has store, copy, drop { item_id: ID }

    /// One can airdrop an item to a Kiosk by calling this function.
    public fun add<T: key + store>(kiosk: &mut Kiosk, item: T) {
        sui::event::emit(ItemDropped<T> {
            kiosk_id: object::id(kiosk),
            item_id: object::id(&item)
        });

        df::add(kiosk::uid_mut(kiosk), AirdropKey<T> {
            item_id: object::id(&item)
        }, item);
    }

    /// Accept an airdropped item and place it into the Kiosk.
    public fun accept<T: key + store>(kiosk: &mut Kiosk, kiosk_cap: &KioskOwnerCap, item_id: ID) {
        let item = df::remove(kiosk::uid_mut(kiosk), AirdropKey<T> { item_id });
        kiosk::place<T>(kiosk, kiosk_cap, item)
    }
}
