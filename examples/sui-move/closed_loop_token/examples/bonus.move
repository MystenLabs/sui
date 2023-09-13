module closed_loop::bonus {
    use std::option;

    use sui::tx_context::{sender, TxContext};
    use sui::object::{Self, UID};
    use sui::transfer;

    use closed_loop::closed_loop::{
        Self as cl, CLPolicy, Join, Split,
        Mint, Burn, Resolver,
        TempToken
    };

    const ONE: u8 = 1;
    const TEN: u8 = 10;
    const FIFTY: u8 = 50;
    const ONE_HUNDRED: u8 = 100;

    const ENoWay: u64 = 22;

    struct BONUS has drop {}

    struct Controller has key {
        id: UID,
        mint_resolver: Resolver<BONUS, Mint>,
        burn_resolver: Resolver<BONUS, Burn>,
    }

    fun init(otw: BONUS, ctx: &mut TxContext) {
        let (cl_bonus, cl_cap) = cl::new_token(otw, ctx);

        let mint_resolver = cl::create_resolver<BONUS, Mint>(
            &cl_cap, &mut cl_bonus, option::none(), option::none(), ctx
        );
        let burn_resolver = cl::create_resolver<BONUS, Burn>(
            &cl_cap, &mut cl_bonus, option::none(), option::none(), ctx
        );
        let ctrl = Controller {
            id: object::new(ctx),
            mint_resolver,
            burn_resolver
        };

        transfer::public_transfer(cl_cap, sender(ctx));
        transfer::public_share_object(cl_bonus);
        transfer::share_object(ctrl);
    }

    fun get_bonus(ctrl: &mut Controller, kind: u8): Token<BONUS> {
        if (kind == ONE) {
            
        } else if (kind == TEN) {

        } else if (kind == FIFTY) {
        } else if (kind == ONE_HUNDRED) {
        } else {
            assert!(false, ENoWay);
        }
}