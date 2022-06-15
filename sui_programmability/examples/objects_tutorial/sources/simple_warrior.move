// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module tutorial::simple_warrior {
    use std::option::{Self, Option};

    use sui::id::VersionedID;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Sword has key, store {
        id: VersionedID,
        strength: u8,
    }

    struct Shield has key, store {
        id: VersionedID,
        armor: u8,
    }

    struct SimpleWarrior has key {
        id: VersionedID,
        sword: Option<Sword>,
        shield: Option<Shield>,
    }

    public entry fun create_sword(strength: u8, ctx: &mut TxContext) {
        let sword = Sword {
            id: tx_context::new_id(ctx),
            strength,
        };
        transfer::transfer(sword, tx_context::sender(ctx))
    }

    public entry fun create_shield(armor: u8, ctx: &mut TxContext) {
        let shield = Shield {
            id: tx_context::new_id(ctx),
            armor,
        };
        transfer::transfer(shield, tx_context::sender(ctx))
    }

    public entry fun create_warrior(ctx: &mut TxContext) {
        let warrior = SimpleWarrior {
            id: tx_context::new_id(ctx),
            sword: option::none(),
            shield: option::none(),
        };
        transfer::transfer(warrior, tx_context::sender(ctx))
    }

    public entry fun equip_sword(warrior: &mut SimpleWarrior, sword: Sword, ctx: &mut TxContext) {
        if (option::is_some(&warrior.sword)) {
            let old_sword = option::extract(&mut warrior.sword);
            transfer::transfer(old_sword, tx_context::sender(ctx));
        };
        option::fill(&mut warrior.sword, sword);
    }

    public entry fun equip_shield(warrior: &mut SimpleWarrior, shield: Shield, ctx: &mut TxContext) {
        if (option::is_some(&warrior.shield)) {
            let old_shield = option::extract(&mut warrior.shield);
            transfer::transfer(old_shield, tx_context::sender(ctx));
        };
        option::fill(&mut warrior.shield, shield);
    }
}
