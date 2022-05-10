// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Games::GiveMeAPowerfulSword {
    use Sui::Coin::{Self, Coin};
    use Sui::SuiObject;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    use Games::Hero::Hero;
    use Games::SeaHero::RUM;
    use Games::SeaHeroHelper::{Self, HelpMeSlayThisMonster};

    struct GiveMeAPowerfulSword has store {
        helper: HelpMeSlayThisMonster,
        helper_owner: address,
        sword_giver_reward: u64
    }

    public fun create(
        helper: HelpMeSlayThisMonster,
        helper_owner: address,
        sword_giver_reward: u64,
        ctx: &mut TxContext,
    ) {
        assert!(
            SeaHeroHelper::owner_reward(&helper) > sword_giver_reward,
            0
        );
        Transfer::transfer(
            SuiObject::create(
                GiveMeAPowerfulSword {
                    helper,
                    helper_owner: TxContext::sender(ctx),
                    sword_giver_reward
                },
                ctx,
            ),
            helper_owner
        )
    }

    public fun slay(
        hero: &Hero, wrapper: GiveMeAPowerfulSword, ctx: &mut TxContext,
    ): Coin<RUM> {
        let GiveMeAPowerfulSword {
            helper,
            helper_owner,
            sword_giver_reward
        } = wrapper;
        let owner_reward = Coin::into_balance(SeaHeroHelper::slay(hero, helper, ctx));
        let sword_giver_reward = Coin::withdraw(&mut owner_reward, sword_giver_reward, ctx);
        Transfer::transfer(Coin::from_balance(owner_reward, ctx), helper_owner);
        sword_giver_reward
    }
}