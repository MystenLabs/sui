// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of a game mod or different game that uses objects from the Hero
/// game.
/// This mod introduces sea monsters that can also be slain with the hero's
/// sword. Instead of boosting the hero's experience, slaying sea monsters
/// earns RUM tokens for hero's owner.
/// Note that this mod does not require special permissions from `Hero` module;
/// anyone is free to create a mod like this.
module games::sea_hero {
    use games::hero::Hero;

    use sui::balance::{Self, Balance, Supply};

    /// Admin capability granting permission to mint RUM tokens and
    /// create monsters
    public struct SeaHeroAdmin has key {
        id: UID,
        /// Permission to mint RUM
        supply: Supply<RUM>,
        /// Total number of monsters created so far
        monsters_created: u64,
        /// cap on the supply of RUM
        token_supply_max: u64,
        /// cap on the number of monsters that can be created
        monster_max: u64
    }

    /// A new kind of monster for the hero to fight
    public struct SeaMonster has key, store {
        id: UID,
        /// Tokens that the user will earn for slaying this monster
        reward: Balance<RUM>
    }

    /// Type of the sea game token
    public struct RUM has drop {}

    // TODO: proper error codes
    /// Hero is not strong enough to defeat the monster. Try healing with a
    /// potion, fighting boars to gain more experience, or getting a better
    /// sword
    const EHERO_NOT_STRONG_ENOUGH: u64 = 0;

    // --- Initialization ---



    #[allow(unused_function)]
    /// Get a treasury cap for the coin and give it to the admin
    // TODO: this leverages Move module initializers
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            SeaHeroAdmin {
                id: object::new(ctx),
                supply: balance::create_supply<RUM>(RUM {}),
                monsters_created: 0,
                token_supply_max: 1000000,
                monster_max: 10,
            },
            ctx.sender()
        )
    }

    // --- Gameplay ---

    /// Slay the `monster` with the `hero`'s sword, earn RUM tokens in
    /// exchange.
    /// Aborts if the hero is not strong enough to slay the monster
    public fun slay(hero: &Hero, monster: SeaMonster): Balance<RUM> {
        let SeaMonster { id, reward } = monster;
        object::delete(id);
        // Hero needs strength greater than the reward value to defeat the
        // monster
        assert!(
            hero.strength() >= reward.value(),
            EHERO_NOT_STRONG_ENOUGH
        );

        reward
    }

    // --- Object and coin creation ---

    /// Game admin can create a monster wrapping a coin worth `reward` and send
    /// it to `recipient`
    public entry fun create_monster(
        admin: &mut SeaHeroAdmin,
        reward_amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let current_coin_supply = admin.supply.supply_value();
        let token_supply_max = admin.token_supply_max;
        // TODO: create error codes
        // ensure token supply cap is respected
        assert!(reward_amount < token_supply_max, 0);
        assert!(token_supply_max - reward_amount >= current_coin_supply, 1);
        // ensure monster supply cap is respected
        assert!(admin.monster_max - 1 >= admin.monsters_created, 2);

        let monster = SeaMonster {
            id: object::new(ctx),
            reward: admin.supply.increase_supply(reward_amount),
        };
        admin.monsters_created = admin.monsters_created + 1;

        transfer::public_transfer(monster, recipient)
    }

    /// Reward a hero will reap from slaying this monster
    public fun monster_reward(monster: &SeaMonster): u64 {
        monster.reward.value()
    }
}
