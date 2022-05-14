// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Games::GiveMeAPowerfulSword {
    use Sui::Coin::{Self, Coin};
    use Sui::SuiObject;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    use Games::Hero::{Self, Hero, Sword};
    use Games::SeaHero::RUM;
    use Games::SeaHeroHelper::{Self, HelpMeSlayThisMonster};

    use Std::Option;

    struct GiveMeAPowerfulSword has store {
        helper: HelpMeSlayThisMonster,
        helper_owner: address,
        sword_giver_reward: u64
    }

    public fun create(
        helper: HelpMeSlayThisMonster,
        hero: Hero, 
        helper_owner: address,
        sword_giver_reward: u64,
        ctx: &mut TxContext,
    ) {
        assert!(
            SeaHeroHelper::owner_reward(&helper) > sword_giver_reward,
            0
        );
        let give_me_sword = GiveMeAPowerfulSword {
            helper,
            helper_owner: TxContext::sender(ctx),
            sword_giver_reward
        };
        /// m
        SuiObject::create_with_child(give_me_sword, hero);
        Transfer::transfer(
            give_me_sword,
            helper_owner
        )
    }

    public fun slay(
        // owned by tx sender
        sword: Sword, 
        // owned by tx sender
        wrapper: SuiObject<GiveMeAPowerfulSword>,
        // owned by SuiObject<GiveMeAPowerfulSword>
        hero: Hero,
        ctx: &mut TxContext,
    ): Coin<RUM> {
        let sender = TxContext::sender(ctx);
        let (give_me_sword, child) = SuiObject::destroy_with_child(wrapper);
        let GiveMeAPowerfulSword {
            helper,
            helper_owner,
            sword_giver_reward
        } = give_me_sword;
        // TODO: delete the child ref

        // give the hero the powerful sword
        let old_sword = Hero::equip_sword(&mut hero, sword);
        let owner_reward = Coin::into_balance(SeaHeroHelper::slay(&hero, helper, ctx));
        let sword_giver_reward = Coin::withdraw(&mut owner_reward, sword_giver_reward, ctx);
        Transfer::transfer(Coin::from_balance(owner_reward, ctx), helper_owner);

        // TODO: re-equip the Hero's old sword (if any). here, just destroy for simplicity
        Option::destroy_none(old_sword);
        // give the sword back to the tx sender
        Transfer::transfer(Hero::remove_sword(&mut hero), sender);
        // give the hero back to its owner
        Transfer::transfer(hero, helper_owner);
        sword_giver_reward
    }
}