// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This Example implements bonus token. 
/// The idea in the example is that an app is tracking user activity
/// and giving bonuses to users when theyhave a certain level of activity.
/// Users of the app (called `Player` in the example) have a value and can
/// use bonuses as multipliers for their value.
/// Actions or operations are not defined in the example but one could imagine
/// that given operations (exposed through given API) can result in value and
/// activity added.
/// Bonuses would then be a way to incentivize action on the app.
module closed_loop::bonus {
    use closed_loop::closed_loop::{
        Self as cl, CLPolicy, CoinIssuerCap, Mint, Burn, Resolver, Token,
    };
    use std::option;
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use sui::transfer;

    /// Internal invariant error, should never happen
    const EInternal: u64 = 100;
    /// Attempt to obtain a toke without enough activity
    const ENotEnoughActivity: u64 = 101;

    // possible bonus tokens
    const BRONZE: u64 = 2;
    const SILVER: u64 = 5;
    const GOLD: u64 = 10;
    const PLATINUM: u64 = 20;

    /// The One Time Witness for the application and the type of the bonus token.
    struct BONUS has drop {}

    /// In the module initializer we create a new `CLPolicy`, share it and set
    /// the rules for interaction following the basic description of what user
    /// can do.
    fun init(otw: BONUS, ctx: &mut TxContext) {
        let (policy, cap) = cl::new_token(otw, ctx);

        let mint_resolver = cl::create_resolver<BONUS, Mint>(
            &cap, &mut policy, option::none(), option::none(), ctx
        );

        let burn_resolver = cl::create_resolver<BONUS, Burn>(
            &cap, &mut policy, option::none(), option::none(), ctx
        );

        // This is our application, created and shared only once.
        let app = App {
            id: object::new(ctx),
            mint_resolver,
            burn_resolver,
            policy,
            cap,
        };

        transfer::share_object(app);
    }

    //
    // Application code
    //

    /// Main App, all operation will go through the App which is a shared object
    struct App has key {
        id: UID,
        /// Required for the creatoin of bonus points.
        mint_resolver: Resolver<BONUS, Mint>,
        /// Required to redeem bonus points.
        burn_resolver: Resolver<BONUS, Burn>,
        /// Token policy
        policy: CLPolicy<BONUS>,
        /// Token capability
        cap: CoinIssuerCap<BONUS>,
    }

    /// A Player is a user of the App. 
    /// Players register via the `join_app` API
    struct Player has key, store {
        id: UID,
        // track player activity
        activity: Activity,
        // track player value
        value: Value,
    }

    /// Track activity in the App.
    /// In this example is just a `dummy` u64
    struct Activity has store, copy, drop {
        dummy: u64,
    }

    /// Track the value of a Player.
    struct Value has store, copy, drop {
        val: u64,
    }

    /// Join the `App` creating a new `Player`
    public fun join_app(_app: &App, ctx: &mut TxContext): Player {
        Player {
            id: object::new(ctx),
            activity: Activity { dummy: 0 },
            value: Value { val: 0 },
        }
    }

    /// A generic `action`. 
    /// This is a simple API intended for testing and it simply increments 
    /// player's activity and value.
    /// In a real application there will be concrete API that represents the 
    /// action a player can make.
    public fun action(_app: &mut App, player: &mut Player, _ctx: &mut TxContext) {
        player.activity.dummy = player.activity.dummy + 1;
        player.value.val = player.value.val + 1;
    }

    /// Perform an action with a bonus token.
    /// The token is a multiplier for the given action.
    public fun bonus_action(app: &mut App, bonus: Token<BONUS>, player: &mut Player, ctx: &mut TxContext) {
        let tmp_token = cl::temp_from_owned(bonus, ctx);
        let burn_req = cl::burn(&mut app.policy, tmp_token, ctx);
        let multiplier = cl::action_value(&burn_req);
        cl::resolve_custom(&mut app.policy, &mut app.burn_resolver, burn_req);
        player.activity.dummy = player.activity.dummy + 1;
        player.value.val = player.value.val * multiplier;
    }

    //
    // API to get given bonuses
    //

    public fun get_bronze_token(app: &mut App, player: &mut Player, ctx: &mut TxContext) {
        get_bonus_token(app, player, BRONZE, ctx)
    }

    public fun get_silver_token(app: &mut App, player: &mut Player, ctx: &mut TxContext) {
        get_bonus_token(app, player, SILVER, ctx)
    }

    public fun get_gold_token(app: &mut App, player: &mut Player, ctx: &mut TxContext) {
        get_bonus_token(app, player, GOLD, ctx)
    }

    public fun get_platinum_token(app: &mut App, player: &mut Player, ctx: &mut TxContext) {
        get_bonus_token(app, player, PLATINUM, ctx)
    }

    //
    // Internal API
    //

    fun get_bonus_token(app: &mut App, player: &mut Player, token_type: u64, ctx: &mut TxContext) {
        assert!(has_activity_for_token(player, token_type), ENotEnoughActivity); 
        let (token, mint_req) = cl::mint(&mut app.policy, token_type, ctx);
        cl::resolve_custom(&mut app.policy, &mut app.mint_resolver, mint_req);
        cl::temp_into_owned(token, ctx);
    }

    fun has_activity_for_token(player: &mut Player, token_type: u64): bool {
        if (token_type == BRONZE) {
            if (player.activity.dummy > 0) {
                player.activity.dummy = player.activity.dummy - 1;
                return true
            }
        } else if (token_type == SILVER) {
            if (player.activity.dummy > 1) {
                player.activity.dummy = player.activity.dummy - 5;
                return true
            }
        } else if (token_type == GOLD) {
            if (player.activity.dummy > 2) {
                player.activity.dummy = player.activity.dummy - 10;
                return true
            }
        } else {
            assert!(token_type == PLATINUM, EInternal);
            if (player.activity.dummy > 3) {
                player.activity.dummy = player.activity.dummy - 20;
                return true
            }
        };
        false
    }

    /// Return the value of a player
    public fun player_value(player: &Player): Value {
        player.value
    }

    /// Return the activity of a player
    public fun player_activity(player: &Player): Activity {
        player.activity
    }

    /// Return the activity value
    public fun get_activity_value(activity: &Activity): u64 {
        activity.dummy
    }

    /// Return the value of a player
    public fun get_value(value: &Value): u64 {
        value.val
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext): App {
        let otw = BONUS {};
        let (policy, cap) = cl::new_token(otw, ctx);
        let mint_resolver = cl::create_resolver<BONUS, Mint>(
            &cap, &mut policy, option::none(), option::none(), ctx
        );
        let burn_resolver = cl::create_resolver<BONUS, Burn>(
            &cap, &mut policy, option::none(), option::none(), ctx
        );
        // This is our application, created and shared only once.
        App {
            id: object::new(ctx),
            mint_resolver,
            burn_resolver,
            policy,
            cap,
        }
    }

    #[test_only]
    public fun destroy_app_for_testing(app: App) {
        let App {
            id,
            mint_resolver: _,
            burn_resolver: _,
            policy,
            cap,
        } = app;
        sui::transfer::public_share_object(policy);
        sui::transfer::public_share_object(cap);

        object::delete(id);
    }
}

#[test_only]
module closed_loop::bonus_tests {
    use sui::tx_context::dummy;
    use closed_loop::bonus;

    #[test] fun test_all() {
        let ctx = &mut dummy();
        let app = bonus::init_for_testing(ctx);
        let player = bonus::join_app(&app, ctx);
        assert!(bonus::get_value(&bonus::player_value(&player)) == 0, 1000);

        bonus::action(&mut app, &mut player, ctx);
        assert!(bonus::get_value(&bonus::player_value(&player)) == 1, 1001);
        assert!(bonus::get_activity_value(&bonus::player_activity(&player)) == 1, 1001);
        bonus::action(&mut app, &mut player, ctx);
        assert!(bonus::get_value(&bonus::player_value(&player)) == 2, 1002);
        assert!(bonus::get_activity_value(&bonus::player_activity(&player)) == 2, 1002);

        // TODO: how do I get the token?
        //       need a different kind of test?
        // bonus::get_bronze_token(&mut app, &mut player, ctx);
        // bonus::bonus_action(&mut app, &mut player, ctx);
        // assert!(bonus::player_value(&player) == 20, 1003);
        // assert!(bonus::player_activity(&player) == 1, 1003);

        sui::transfer::public_share_object(player);
        bonus::destroy_app_for_testing(app);
    }
}
