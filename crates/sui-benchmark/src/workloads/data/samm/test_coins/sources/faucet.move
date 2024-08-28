module test::faucet {
    use std::ascii::String;
    use std::type_name::{get, into_string};

    use sui::bag::{Self, Bag};
    use sui::balance::{Self, Supply};
    use sui::coin::{Self, TreasuryCap, Coin};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_set::{Self, VecSet};

    use swap::implements::Global;
    use swap::interface::add_liquidity;
    use test::coins::{get_coins, BTC, USDT, ETH, BNB, USDC, DAI};

    const ONE_COIN: u64 = 100000000;

    const ERR_NO_PERMISSIONS: u64 = 1;
    const ERR_NOT_ENOUGH_COINS: u64 = 2;

    struct Faucet has key {
        id: UID,
        coins: Bag,
        creator: address,
        admins: VecSet<address>
    }

    fun init(
        ctx: &mut TxContext
    ) {
        let admin = @0xc05eaaf1369ece51ce0b8ad5cb797b737d4f2eba;
        let admins = vec_set::empty<address>();
        vec_set::insert(&mut admins, admin);

        transfer::share_object(
            Faucet {
                id: object::new(ctx),
                coins: get_coins(ctx),
                creator: tx_context::sender(ctx),
                admins
            }
        )
    }

    public entry fun add_admin(
        faucet: &mut Faucet,
        new_admin: address,
        ctx: &mut TxContext
    ) {
        assert!(faucet.creator == tx_context::sender(ctx), ERR_NO_PERMISSIONS);
        vec_set::insert(&mut faucet.admins, new_admin)
    }

    public entry fun remove_admin(
        faucet: &mut Faucet,
        old_admin: address,
        ctx: &mut TxContext
    ) {
        assert!(faucet.creator == tx_context::sender(ctx), ERR_NO_PERMISSIONS);
        vec_set::remove(&mut faucet.admins, &old_admin)
    }

    public entry fun add_supply<T>(
        faucet: &mut Faucet,
        treasury_cap: TreasuryCap<T>,
    ) {
        let supply = coin::treasury_into_supply(treasury_cap);

        bag::add(
            &mut faucet.coins,
            into_string(get<T>()),
            supply
        )
    }

    fun mint_coins<T>(
        faucet: &mut Faucet,
        amount: u64,
        ctx: &mut TxContext
    ): Coin<T> {
        let coin_name = into_string(get<T>());
        assert!(
            bag::contains_with_type<String, Supply<T>>(&faucet.coins, coin_name),
            ERR_NOT_ENOUGH_COINS
        );

        let mut_supply = bag::borrow_mut<String, Supply<T>>(
            &mut faucet.coins,
            coin_name
        );

        let minted_balance = balance::increase_supply(
            mut_supply,
            amount * ONE_COIN
        );

        coin::from_balance(minted_balance, ctx)
    }

    public entry fun claim<T>(
        faucet: &mut Faucet,
        ctx: &mut TxContext,
    ) {
        transfer::public_transfer(
            mint_coins<T>(faucet, 1, ctx),
            tx_context::sender(ctx)
        )
    }

    public entry fun force_claim<T>(
        faucet: &mut Faucet,
        amount: u64,
        ctx: &mut TxContext,
    ) {
        let operator = tx_context::sender(ctx);
        assert!(
            faucet.creator == operator
                || vec_set::contains(&faucet.admins, &operator),
            ERR_NO_PERMISSIONS
        );

        transfer::public_transfer(
            mint_coins<T>(faucet, amount, ctx),
            operator
        )
    }

    public entry fun force_add_liquidity(
        faucet: &mut Faucet,
        global: &mut Global,
        ctx: &mut TxContext,
    ) {
        let operator = tx_context::sender(ctx);
        assert!(
            faucet.creator == operator
                || vec_set::contains(&faucet.admins, &operator),
            ERR_NO_PERMISSIONS
        );

        // BTC-ETH
        // BTC: 10000, ETH: 100000
        let coin_btc = mint_coins<BTC>(faucet, 10000, ctx);
        let coin_eth = mint_coins<ETH>(faucet, 100000, ctx);
        add_liquidity<BTC, ETH>(global, coin_btc, 1, coin_eth, 1, ctx);

        // BTC-USDT
        // BTC: 10000, USDT: 1000000
        let coin_btc = mint_coins<BTC>(faucet, 10000, ctx);
        let coin_usdt = mint_coins<USDT>(faucet, 1000000, ctx);
        add_liquidity<BTC, USDT>(global, coin_btc, 1, coin_usdt, 1, ctx);

        // ETH-USDT
        // ETH: 100000, USDT: 1000000
        let coin_eth = mint_coins<ETH>(faucet, 100000, ctx);
        let coin_usdt = mint_coins<USDT>(faucet, 1000000, ctx);
        add_liquidity<ETH, USDT>(global, coin_eth, 1, coin_usdt, 1, ctx);

        // USDC-USDT
        // USDC: 1000000, USDT: 1000000
        let coin_usdc = mint_coins<USDC>(faucet, 1000000, ctx);
        let coin_usdt = mint_coins<USDT>(faucet, 1000000, ctx);
        add_liquidity<USDC, USDT>(global, coin_usdc, 1, coin_usdt, 1, ctx);

        // BNB-USDT
        // BNB: 100000, USDT: 1000000
        let coin_bnb = mint_coins<BNB>(faucet, 100000, ctx);
        let coin_usdt = mint_coins<USDT>(faucet, 1000000, ctx);
        add_liquidity<BNB, USDT>(global, coin_bnb, 1, coin_usdt, 1, ctx);

        // BNB-USDC
        // BNB: 100000, USDC: 1000000
        let coin_bnb = mint_coins<BNB>(faucet, 100000, ctx);
        let coin_usdc = mint_coins<USDC>(faucet, 1000000, ctx);
        add_liquidity<BNB, USDC>(global, coin_bnb, 1, coin_usdc, 1, ctx);

        // DAI-USDC
        // DAI: 1000000, USDC: 1000000
        let coin_dai = mint_coins<DAI>(faucet, 1000000, ctx);
        let coin_usdc = mint_coins<USDC>(faucet, 1000000, ctx);
        add_liquidity<DAI, USDC>(global, coin_dai, 1, coin_usdc, 1, ctx);
        
        // BTC-DAI
        // BTC: 10000, DAI: 1000000
        let coin_btc = mint_coins<BTC>(faucet, 10000, ctx);
        let coin_dai = mint_coins<DAI>(faucet, 1000000, ctx);
        add_liquidity<BTC, DAI>(global, coin_btc, 1, coin_dai, 1, ctx);

        // DAI-ETH
        // DAI: 1000000, ETH: 100000
        let coin_dai = mint_coins<DAI>(faucet, 1000000, ctx);
        let coin_eth = mint_coins<ETH>(faucet, 100000, ctx);
        add_liquidity(global, coin_dai, 1, coin_eth, 1, ctx);
    }
}
