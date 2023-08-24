// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This Example implements loyalty points. The points are earned by the customer
/// when they perform an incentivized action (e.g. purchase a product). The points
/// are not transferable, however, they can be joined and split and can be spent
/// on a product or service.
module closed_loop::loyalty {
    use std::option;

    use sui::tx_context::{sender, TxContext};
    use sui::object::{Self, UID};
    use sui::transfer;

    use closed_loop::closed_loop::{
        Self as cl, CLPolicy, Join, Split,
        Mint, Burn, Resolver,
        TempToken
    };

    /// Error code for the case when the user does not have enough points.
    const ENotEnough: u64 = 0;

    /// The amount of reward for the incentivized action.
    const REWARD_AMOUNT: u64 = 100;
    /// Let's make it so the user needs to do 2 actions to get a "plushy bara".
    const PLUSHY_BARA_PRICE: u64 = 200;

    /// The One Time Witness for the application and the type of the closed loop
    /// token.
    struct LOYALTY has drop {}

    // Closed Loop token allows its creator to set custom resolvers and integrate
    // them into an application. So we need an application :)

    /// The central place in the Loyalty program. Shall we rename it? Let's call
    /// it App to make more generic and less marketplace-y.
    struct App has key {
        id: UID,
        /// Special resolvers for the application. Allows minting Tokens.
        mint_resolver: Resolver<LOYALTY, Mint>,
        /// This resolver will help us deal with purchases.
        burn_resolver: Resolver<LOYALTY, Burn>,
    }

    /// Freely tradable and transferable object. Can only be purchased for X
    /// amount of loyalty points.
    struct PlushyBara has key, store {
        id: UID
    }

    /// In the module initializer we create a new `CLPolicy`, share it and set
    /// the rules for interaction following the basic description of what user
    /// can do.
    fun init(otw: LOYALTY, ctx: &mut TxContext) {
        let (cl_policy, cl_cap) = cl::new_token(otw, ctx);

        // User can Join and Split tokens into one.
        cl::allow<LOYALTY, Join>(&cl_cap, &mut cl_policy);
        cl::allow<LOYALTY, Split>(&cl_cap, &mut cl_policy);

        let mint_resolver = cl::create_resolver<LOYALTY, Mint>(
            &cl_cap, &mut cl_policy, option::none(), option::none(), ctx
        );

        let burn_resolver = cl::create_resolver<LOYALTY, Burn>(
            &cl_cap, &mut cl_policy, option::none(), option::none(), ctx
        );

        // This is our application, created and shared only once.
        let app = App {
            id: object::new(ctx),
            mint_resolver,
            burn_resolver
        };

        transfer::public_transfer(cl_cap, sender(ctx));
        transfer::public_share_object(cl_policy);
        transfer::share_object(app); // type is defined in the same module!
    }

    /// Dummy function to get rewards.
    /// The sender receives a TempToken which they can only merge with their
    /// tokens or simply turn into an OwnedToken version which is not transferable.
    ///
    /// Returning it to user makes the design simpler, and it actually makes
    /// more sense as the only action a user can perform is just storing.
    public fun incentivized_action(
        app: &mut App, policy: &mut CLPolicy<LOYALTY>, ctx: &mut TxContext
    ): TempToken<LOYALTY> {

        // On every action in the `closed_loop` module we get the result of this
        // action and a request which needs to be resolved.
        let (token, mint_req) = cl::mint(policy, REWARD_AMOUNT, ctx);

        // Resolve requests using protected `Resolver`s in the App.
        // `Join` and `Split` can be resolved without custom resolvers.
        cl::resolve_custom(policy, &mut app.mint_resolver, mint_req);

        token
    }

    /// Let's look at the signature first. Given that we create a new object, we
    /// definitely need a context.
    ///
    /// With that being done, we need to figure out how to spend the LOYALTY tokens.
    public fun claim_plushy_bara(
        // App holds our resolvers, we need it.
        app: &mut App,
        // Policy is &mut because we burn Tokens on every spend
        policy: &mut CLPolicy<LOYALTY>,
        // Finally, we need to pass the tokens.
        tokens: TempToken<LOYALTY>,
        ctx: &mut TxContext
    ): PlushyBara {
        assert!(cl::value(&tokens) == PLUSHY_BARA_PRICE, ENotEnough);

        // How do we deal with this request?..
        let burn_req = cl::burn(policy, tokens, ctx);

        // done.
        cl::resolve_custom(policy, &mut app.burn_resolver, burn_req);

        PlushyBara {
            id: object::new(ctx)
        }
    }

    #[test_only]
    // getting this buddy only for tests
    use closed_loop::closed_loop::CoinIssuerCap;

    #[test_only]
    // Ugly, I know, but we're cutting corners here.
    public fun init_for_testing(ctx: &mut TxContext): (
        CLPolicy<LOYALTY>, CoinIssuerCap<LOYALTY>, App
    ) {
        let otw = LOYALTY {};
        let (cl_policy, cl_cap) = cl::new_token(otw, ctx);

        // With the preparations done, we can set what user can do:
        cl::allow<LOYALTY, Join>(&cl_cap, &mut cl_policy);
        cl::allow<LOYALTY, Split>(&cl_cap, &mut cl_policy);

        let mint_resolver = cl::create_resolver<LOYALTY, Mint>(
            &cl_cap, &mut cl_policy, option::none(), option::none(), ctx
        );

        let burn_resolver = cl::create_resolver<LOYALTY, Burn>(
            &cl_cap, &mut cl_policy, option::none(), option::none(), ctx
        );

        // This is our application, created and shared only once.
        let app = App {
            id: object::new(ctx),
            mint_resolver,
            burn_resolver
        };

        (cl_policy, cl_cap, app)
    }

    #[test_only]
    public fun destroy_app_for_testing(app: App) {
        let App {
            id,
            mint_resolver: _,
            burn_resolver: _
        } = app;

        object::delete(id);
    }
}

// Now that we have it working, it's time we wrote some tests!
#[test_only]
module closed_loop::loyalty_tests {
    use sui::tx_context::{sender, dummy};
    use closed_loop::closed_loop as cl;
    use closed_loop::loyalty;

    #[test] fun receive_and_claim() {
        let ctx = &mut dummy();
        let (policy, cap, app) = loyalty::init_for_testing(ctx);

        // great, it's working. Let's try to get some points.
        let reward_1 = loyalty::incentivized_action(&mut app, &mut policy, ctx);
        let reward_2 = loyalty::incentivized_action(&mut app, &mut policy, ctx);

        // now we need to join rewards together. It's a restricted action, so...
        let join_req = cl::join(&mut reward_1, reward_2, ctx);
        assert!(cl::value(&reward_1) == 200, 0);

        // first we need to deal with request
        cl::resolve_default(&mut policy, join_req);

        // now we can claim our plushy bara
        let plushy = loyalty::claim_plushy_bara(&mut app, &mut policy, reward_1, ctx);

        // did it work? let's deal with objects first.
        sui::transfer::public_share_object(policy);
        sui::transfer::public_share_object(cap);
        loyalty::destroy_app_for_testing(app);

        // finally, if everything worked out, we can transfer the 'Bara to the
        // winner!

        sui::transfer::public_transfer(plushy, sender(ctx));
    }
}
