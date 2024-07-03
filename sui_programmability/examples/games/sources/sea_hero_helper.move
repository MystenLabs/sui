// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Mod of the economics of the SeaHero game. In the game, a `Hero` can only
/// slay a `SeaMonster` if they have sufficient strength. This mod allows a
/// player with a weak `Hero` to ask a player with a stronger `Hero` to slay
/// the monster for them in exchange for some of the reward.
/// Anyone can create a mod like this--the permission of the `SeaHero` game
/// is not required.
module games::sea_hero_helper {
    use games::sea_hero::{Self, SeaMonster, RUM};
    use games::hero::Hero;
    use sui::coin::{Self, Coin};

    /// Created by `monster_owner`, a player with a monster that's too strong
    /// for them to slay + transferred to a player who can slay the monster.
    /// The two players split the reward for slaying the monster according to
    /// the `helper_reward` parameter.
    public struct HelpMeSlayThisMonster has key {
        id: UID,
        /// Monster to be slay by the owner of this object
        monster: SeaMonster,
        /// Identity of the user that originally owned the monster
        monster_owner: address,
        /// Number of tokens that will go to the helper. The owner will get
        /// the `monster` reward - `helper_reward` tokens
        helper_reward: u64,
    }

    // TODO: proper error codes
    /// The specified helper reward is too large
    const EINVALID_HELPER_REWARD: u64 = 0;

    /// Create an offer for `helper` to slay the monster in exchange for
    /// some of the reward
    public fun create(
        monster: SeaMonster,
        helper_reward: u64,
        helper: address,
        ctx: &mut TxContext,
    ) {
        // make sure the advertised reward is not too large + that the owner
        // gets a nonzero reward
        assert!(
            monster.monster_reward() > helper_reward,
            EINVALID_HELPER_REWARD
        );
        transfer::transfer(
            HelpMeSlayThisMonster {
                id: object::new(ctx),
                monster,
                monster_owner: ctx.sender(),
                helper_reward
            },
            helper
        )
    }

    /// Helper should call this if they are willing to help out and slay the
    /// monster.
    public fun slay(
        hero: &Hero, wrapper: HelpMeSlayThisMonster, ctx: &mut TxContext,
    ): Coin<RUM> {
        let HelpMeSlayThisMonster {
            id,
            monster,
            monster_owner,
            helper_reward
        } = wrapper;
        object::delete(id);
        let mut owner_reward = sea_hero::slay(hero, monster);
        let helper_reward = coin::take(&mut owner_reward, helper_reward, ctx);
        transfer::public_transfer(owner_reward.into_coin(ctx), monster_owner);
        helper_reward
    }

    /// Helper can call this if they can't help slay the monster or don't want
    /// to, and are willing to kindly return the monster to its owner.
    public fun return_to_owner(wrapper: HelpMeSlayThisMonster) {
        let HelpMeSlayThisMonster {
            id,
            monster,
            monster_owner,
            helper_reward: _
        } = wrapper;
        object::delete(id);
        transfer::public_transfer(monster, monster_owner)
    }

    /// Return the number of coins that `wrapper.owner` will earn if the
    /// the helper slays the monster in `wrapper.
    public fun owner_reward(wrapper: &HelpMeSlayThisMonster): u64 {
        wrapper.monster.monster_reward() - wrapper.helper_reward
    }
}
