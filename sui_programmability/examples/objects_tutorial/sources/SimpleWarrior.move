// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Tutorial::SimpleWarrior {
    use Std::Option::{Self, Option};

    use Sui::ID::VersionedID;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

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

    public(script) fun create_sword(strength: u8, ctx: &mut TxContext) {
        let sword = Sword {
            id: TxContext::new_id(ctx),
            strength,
        };
        Transfer::transfer(sword, TxContext::sender(ctx))
    }

    public(script) fun create_shield(armor: u8, ctx: &mut TxContext) {
        let shield = Shield {
            id: TxContext::new_id(ctx),
            armor,
        };
        Transfer::transfer(shield, TxContext::sender(ctx))
    }

    public(script) fun create_warrior(ctx: &mut TxContext) {
        let warrior = SimpleWarrior {
            id: TxContext::new_id(ctx),
            sword: Option::none(),
            shield: Option::none(),
        };
        Transfer::transfer(warrior, TxContext::sender(ctx))
    }

    public(script) fun equip_sword(warrior: &mut SimpleWarrior, sword: Sword, ctx: &mut TxContext) {
        if (Option::is_some(&warrior.sword)) {
            let old_sword = Option::extract(&mut warrior.sword);
            Transfer::transfer(old_sword, TxContext::sender(ctx));
        };
        Option::fill(&mut warrior.sword, sword);
    }

    public(script) fun equip_shield(warrior: &mut SimpleWarrior, shield: Shield, ctx: &mut TxContext) {
        if (Option::is_some(&warrior.shield)) {
            let old_shield = Option::extract(&mut warrior.shield);
            Transfer::transfer(old_shield, TxContext::sender(ctx));
        };
        Option::fill(&mut warrior.shield, shield);
    }
}