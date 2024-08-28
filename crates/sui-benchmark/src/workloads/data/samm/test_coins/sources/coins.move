module test::coins {
    use std::type_name::{into_string, get};

    use sui::bag::{Self, Bag};
    use sui::balance;
    use sui::tx_context::TxContext;

    friend test::faucet;

    ////////////////////////////////////
    struct USDT has drop {}

    struct XBTC has drop {}

    struct BTC has drop {}

    struct ETH has drop {}

    struct BNB has drop {}

    struct WBTC has drop {}

    struct USDC has drop {}

    struct DAI has drop {}

    ////////////////////////////////////

    public(friend) fun get_coins(ctx: &mut TxContext): Bag {
        let coins = bag::new(ctx);

        bag::add(&mut coins, into_string(get<USDT>()), balance::create_supply(USDT {}));
        bag::add(&mut coins, into_string(get<XBTC>()), balance::create_supply(XBTC {}));
        bag::add(&mut coins, into_string(get<BTC>()), balance::create_supply(BTC {}));
        bag::add(&mut coins, into_string(get<ETH>()), balance::create_supply(ETH {}));
        bag::add(&mut coins, into_string(get<BNB>()), balance::create_supply(BNB {}));
        bag::add(&mut coins, into_string(get<WBTC>()), balance::create_supply(WBTC {}));
        bag::add(&mut coins, into_string(get<USDC>()), balance::create_supply(USDC {}));
        bag::add(&mut coins, into_string(get<DAI>()), balance::create_supply(DAI {}));

        coins
    }
}
