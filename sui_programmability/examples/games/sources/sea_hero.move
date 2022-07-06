// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Example of a game mod or different game that uses objects from the Hero
/// game.
/// This mod introduces sea monsters that can also be slain with the hero's
/// sword. Instead of boosting the hero's experience, slaying sea monsters
/// earns RUM tokens for hero's owner.
/// Note that this mod does not require special permissions from `Hero` module;
/// anyone is free to create a mod like this.
module games::sea_hero {
    use games::hero::{Self, Hero};

    use sui::balance::{Self, Balance, Supply};
    use sui::id::{Self, VersionedID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Admin capability granting permission to mint RUM tokens and
    /// create monsters
    struct SeaHeroAdmin has key {
        id: VersionedID,
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
    struct SeaMonster has key, store {
        id: VersionedID,
        /// Tokens that the user will earn for slaying this monster
        reward: Balance<RUM>
    }

    /// Type of the sea game token
    struct RUM has drop {}

    // TODO: proper error codes
    /// Hero is not strong enough to defeat the monster. Try healing with a
    /// potion, fighting boars to gain more experience, or getting a better
    /// sword
    const EHERO_NOT_STRONG_ENOUGH: u64 = 0;
    /// Too few tokens issued
    const EINVALID_TOKEN_SUPPLY: u64 = 1;
    /// Too few monsters created
    const EINVALID_MONSTER_SUPPLY: u64 = 2;

    // --- Initialization ---



    /// Get a treasury cap for the coin and give it to the admin
    // TODO: this leverages Move module initializers
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            SeaHeroAdmin {
                id: tx_context::new_id(ctx),
                supply: balance::create_supply<RUM>(RUM {}),
                monsters_created: 0,
                token_supply_max: 1000000,
                monster_max: 10,
            },
            tx_context::sender(ctx)
        )
    }

    // --- Gameplay ---

    /// Slay the `monster` with the `hero`'s sword, earn RUM tokens in
    /// exchange.
    /// Aborts if the hero is not strong enough to slay the monster
    public fun slay(hero: &Hero, monster: SeaMonster): Balance<RUM> {
        let SeaMonster { id, reward } = monster;
        id::delete(id);
        // Hero needs strength greater than the reward value to defeat the
        // monster
        assert!(
            hero::hero_strength(hero) >= balance::value(&reward),
            EHERO_NOT_STRONG_ENOUGH
        );

        reward
    }

    // --- Object and coin creation ---

    /// Game admin can reate a monster wrapping a coin worth `reward` and send
    /// it to `recipient`
    public entry fun create_monster(
        admin: &mut SeaHeroAdmin,
        reward_amount: u64,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let current_coin_supply = balance::supply_value(&admin.supply);
        let token_supply_max = admin.token_supply_max;
        // TODO: create error codes
        // ensure token supply cap is respected
        assert!(reward_amount < token_supply_max, 0);
        assert!(token_supply_max - reward_amount >= current_coin_supply, 1);
        // ensure monster supply cap is respected
        assert!(admin.monster_max - 1 >= admin.monsters_created, 2);

        let monster = SeaMonster {
            id: tx_context::new_id(ctx),
            reward: balance::increase_supply(&mut admin.supply, reward_amount),
        };
        admin.monsters_created = admin.monsters_created + 1;

        transfer::transfer(monster, recipient)
    }

    /// Send `monster` to `recipient`
    public fun transfer_monster(
        monster: SeaMonster, recipient: address
    ) {
        transfer::transfer(monster, recipient)
    }

    /// Reward a hero will reap from slaying this monster
    public fun monster_reward(monster: &SeaMonster): u64 {
        balance::value(&monster.reward)
    }
}
