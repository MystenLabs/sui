module amm::util {
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use amm::pool::{Self, PoolRegistry, LP, Pool, AdminCap};

    /// Destroys the provided balance if zero, otherwise converts it to a `Coin`
    /// and transfers it to recipient.
    fun destroy_or_transfer_balance<T>(balance: Balance<T>, recipient: address, ctx: &mut TxContext) {
        if (balance::value(&balance) == 0) {
            balance::destroy_zero(balance);
            return
        };
        transfer::public_transfer(
            coin::from_balance(balance, ctx),
            recipient
        );
    }

    /// Destroys the provided balance if zero, otherwise transfers it to recipient.
    fun destroy_or_transfer_coin<T>(coin: Coin<T>, recipient: address) {
        if (coin::value(&coin) == 0) {
            coin::destroy_zero(coin);
            return
        };
        transfer::public_transfer(
            coin,
            recipient
        );
    }

    /// Calls `pool::create` using Coins as input. Returns the resulting LP Coin.
    public fun create_pool_with_coins<A, B>(
        registry: &mut PoolRegistry,
        init_a: Coin<A>,
        init_b: Coin<B>,
        lp_fee_bps: u64,
        admin_fee_pct: u64,
        ctx: &mut TxContext,
    ): Coin<LP<A, B>> {
        let lp_balance = pool::create(
            registry,
            coin::into_balance(init_a),
            coin::into_balance(init_b),
            lp_fee_bps,
            admin_fee_pct,
            ctx
        );

        coin::from_balance(lp_balance, ctx)
    }

    /// Calls `pool::create` using Coins as input. Transfers the resulting LP Coin
    /// to the sender.
    public fun create_pool_and_transfer_lp_to_sender<A, B>(
        registry: &mut PoolRegistry,
        init_a: Coin<A>,
        init_b: Coin<B>,
        lp_fee_bps: u64,
        admin_fee_pct: u64,
        ctx: &mut TxContext,
    ) {
        let lp_balance = pool::create(
            registry,
            coin::into_balance(init_a),
            coin::into_balance(init_b),
            lp_fee_bps,
            admin_fee_pct,
            ctx
        );

        transfer::public_transfer(
            coin::from_balance(lp_balance, ctx),
            tx_context::sender(ctx)
        )
    }

    /// Calls `pool::deposit` using Coins as input. Returns the remainder of the input
    /// Coins and the LP Coin of appropriate value.
    public fun deposit_coins<A, B>(
        pool: &mut Pool<A, B>,
        input_a: Coin<A>,
        input_b: Coin<B>,
        min_lp_out: u64,
        ctx: &mut TxContext
    ): (Coin<A>, Coin<B>, Coin<LP<A, B>>) {
        let (remaining_a, remaining_b, lp) = pool::deposit(
            pool, coin::into_balance(input_a), coin::into_balance(input_b), min_lp_out
        );

        (
            coin::from_balance(remaining_a, ctx),
            coin::from_balance(remaining_b, ctx),
            coin::from_balance(lp, ctx)
        )
    }

    /// Calls `pool::deposit` using Coins as input. Transfers the remainder of the input
    /// Coins and the LP Coin of appropriate value to the sender.
    public fun deposit_and_transfer_to_sender<A, B>(
        pool: &mut Pool<A, B>,
        input_a: Coin<A>,
        input_b: Coin<B>,
        min_lp_out: u64,
        ctx: &mut TxContext
    ) {
        let (remaining_a, remaining_b, lp) = pool::deposit(
            pool, coin::into_balance(input_a), coin::into_balance(input_b), min_lp_out
        );

        // transfer the output amounts to the caller (if any)
        let sender = tx_context::sender(ctx);
        destroy_or_transfer_balance(remaining_a, sender, ctx);
        destroy_or_transfer_balance(remaining_b, sender, ctx);
        destroy_or_transfer_balance(lp, sender, ctx);
    }

    /// Calls `pool::withdraw` using Coin as input. Returns the withdrawn Coins.
    public fun withdraw_coins<A, B>(
        pool: &mut Pool<A, B>,
        lp_in: Coin<LP<A, B>>,
        min_a_out: u64,
        min_b_out: u64,
        ctx: &mut TxContext
    ): (Coin<A>, Coin<B>) {
        let (a_out, b_out) = pool::withdraw(
            pool, coin::into_balance(lp_in), min_a_out, min_b_out
        );

        (coin::from_balance(a_out, ctx), coin::from_balance(b_out, ctx))
    }

    /// Calls `pool::withdraw` using Coin as input. Transfers the withdrawn Coins to the sender.
    public fun withdraw_and_transfer_to_sender<A, B>(
        pool: &mut Pool<A, B>,
        lp_in: Coin<LP<A, B>>,
        min_a_out: u64,
        min_b_out: u64,
        ctx: &mut TxContext
    ) {
        let (a_out, b_out) = pool::withdraw(pool, coin::into_balance(lp_in), min_a_out, min_b_out);

        let sender = tx_context::sender(ctx);
        destroy_or_transfer_balance(a_out, sender, ctx);
        destroy_or_transfer_balance(b_out, sender, ctx);
    }

    /// Calls `pool::swap_a` using Coin as input. Returns the resulting Coin.
    public fun swap_a_coin<A, B>(
        pool: &mut Pool<A, B>, input: Coin<A>, min_out: u64, ctx: &mut TxContext
    ): Coin<B> {
        let out = pool::swap_a(pool, coin::into_balance(input), min_out);
        coin::from_balance(out, ctx)
    }

    /// Calls `pool::swap_a` using Coin as input. Transfers the resulting Coin to the sender.
    public fun swap_a_and_transfer_to_sender<A, B>(
        pool: &mut Pool<A, B>, input: Coin<A>, min_out: u64, ctx: &mut TxContext
    ) {
        let out = pool::swap_a(pool, coin::into_balance(input), min_out);
        destroy_or_transfer_balance(out, tx_context::sender(ctx), ctx);
    }

    /// Calls `pool::swap_b` using Coin as input. Returns the resulting Coin.
    public fun swap_b_coin<A, B>(
        pool: &mut Pool<A, B>, input: Coin<B>, min_out: u64, ctx: &mut TxContext
    ): Coin<A> {
        let out = pool::swap_b(pool, coin::into_balance(input), min_out);
        coin::from_balance(out, ctx)
    }

    /// Calls `pool::swap_b` using Coin as input. Transfers the resulting Coin to the sender.
    public fun swap_b_and_transfer_to_sender<A, B>(
        pool: &mut Pool<A, B>, input: Coin<B>, min_out: u64, ctx: &mut TxContext
    ) {
        let out = pool::swap_b(pool, coin::into_balance(input), min_out);
        destroy_or_transfer_balance(out, tx_context::sender(ctx), ctx);
    }

    /// Calls `pool::admin_withdraw_fees`. Returns the withdrawn fees as Coin.
    public fun admin_withdraw_fees_coin<A, B>(
        pool: &mut Pool<A, B>,
        admin_cap: &AdminCap,
        amount: u64,
        ctx: &mut TxContext
    ): Coin<LP<A, B>> {
        let lp = pool::admin_withdraw_fees(pool, admin_cap, amount);
        coin::from_balance(lp, ctx)
    }

    /// Calls `pool::admin_withdraw_fees`. Transfers the withdrawn fees to sender.
    public fun admin_withdraw_fees_and_transfer_to_sender<A, B>(
        pool: &mut Pool<A, B>,
        admin_cap: &AdminCap,
        amount: u64,
        ctx: &mut TxContext
    ) {
        let lp = pool::admin_withdraw_fees(pool, admin_cap, amount);
        destroy_or_transfer_balance(lp, tx_context::sender(ctx), ctx);
    }
}