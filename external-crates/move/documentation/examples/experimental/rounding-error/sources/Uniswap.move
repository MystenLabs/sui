module NamedAddr::Uniswap {

    const PoolAddr: address = @NamedAddr;

    struct Pool has key {
        coin1: u64,
        coin2: u64,
        total_share: u64
    }

    fun getInputPrice(input_amount: u64, input_reserve: u64, output_reserve: u64): u64 {
        let input_amount_with_fee = input_amount * 997;
        let numerator = input_amount_with_fee * output_reserve;
        let denominator = (input_reserve * 1000) + input_amount_with_fee;
        numerator / denominator
    }
    public fun coin1_to_coin2_swap_code(coin1_in: u64): u64 acquires Pool {
        let pool = borrow_global_mut<Pool>(PoolAddr);
        let coin2_out = getInputPrice(coin1_in, pool.coin1, pool.coin2);
        pool.coin1 = pool.coin1 + coin1_in;
        pool.coin2 = pool.coin2 - coin2_out;
        coin2_out
    }
    spec coin1_to_coin2_swap_code {
        let old_pool = global<Pool>(PoolAddr);
        let post new_pool = global<Pool>(PoolAddr);
        ensures old_pool.coin1 * old_pool.coin2 <= new_pool.coin1 * new_pool.coin2; // safety property
    }

    // https://hackmd.io/@HaydenAdams/HJ9jLsfTz?type=view#ETH-%E2%87%84-ERC20-Trades
    public fun coin1_to_coin2_swap_whitepaper(coin1_in: u64): u64 acquires Pool {
        assert! (coin1_in > 0, 1);
        let fee = coin1_in * 3 / 1000; // 0.3%
        let pool = borrow_global_mut<Pool>(PoolAddr);
        spec {
            assume pool.coin1 > 0;
            assume pool.coin2 > 0;
            assume pool.coin1 == 1337;
            assume pool.coin2 == 252;
        };
        let inv = pool.coin1 * pool.coin2; // `inv` may need to be u128
        let new_pool_coin1 = pool.coin1 + coin1_in;
        let new_pool_coin2 = inv / (new_pool_coin1 - fee); // No div-by-zero because (new_pool_coin1 - fee) cannot be 0.
        let coin2_out = pool.coin2 - new_pool_coin2;
        pool.coin1 = new_pool_coin1;
        pool.coin2 = new_pool_coin2;
        coin2_out
    }
    spec coin1_to_coin2_swap_whitepaper {
        let old_pool = global<Pool>(PoolAddr);
        let post new_pool = global<Pool>(PoolAddr);
        ensures old_pool.coin1 < new_pool.coin1;
        ensures old_pool.coin2 >= new_pool.coin2;
        ensures old_pool.coin1 * old_pool.coin2 <= new_pool.coin1 * new_pool.coin2;
    }

    public fun add_liquidity(coin1_in: u64, coin2_in: u64): u64 // returns liquidity share
    acquires Pool {
        let pool = borrow_global_mut<Pool>(PoolAddr);

        let coin1_added = coin1_in;
        let share_minted = (coin1_added * pool.total_share) / pool.coin1;
        let coin2_added = (share_minted * pool.coin2) / pool.total_share;
        // let coin2_added = (coin1_added  * pool.coin2 ) / pool.coin1; // alternatively ...

        assert!(coin2_in == coin2_added, 1);

        pool.coin1 = pool.coin1 + coin1_added;
        pool.coin2 = pool.coin2 + coin2_added;
        pool.total_share = pool.total_share + share_minted;

        share_minted
    }
    spec add_liquidity {
        let old_pool = global<Pool>(PoolAddr);
        let post new_pool = global<Pool>(PoolAddr);
        ensures old_pool.coin1 <= new_pool.coin1;
        ensures old_pool.coin2 <= new_pool.coin2;
        ensures old_pool.total_share <= new_pool.total_share;
        ensures old_pool.coin1 * old_pool.coin2 <= new_pool.coin1 * new_pool.coin2;
    }

    public fun remove_liquidity(share: u64): (u64, u64) // returns (coin1, coin2)
    acquires Pool {
        let pool = borrow_global_mut<Pool>(PoolAddr);

        let coin1_removed = (pool.coin1 * share) / pool.total_share;
        let coin2_removed = (pool.coin2 * share) / pool.total_share;

        pool.coin1 = pool.coin1 - coin1_removed;
        pool.coin2 = pool.coin2 - coin2_removed;
        pool.total_share = pool.total_share - share;

        (coin1_removed, coin2_removed)
    }
    spec remove_liquidity {
        pragma verify=false;
        let old_pool = global<Pool>(PoolAddr);
        let post new_pool = global<Pool>(PoolAddr);
        ensures old_pool.coin1 <= new_pool.coin1;
        ensures old_pool.coin2 <= new_pool.coin2;
        ensures old_pool.total_share <= new_pool.total_share;
        ensures old_pool.coin1 * old_pool.coin2 <= new_pool.coin1 * new_pool.coin2;
    }

    // #[test] // TODO: cannot specify the test-only functions
    fun no_free_money_theorem(coin1_in: u64, coin2_in: u64): (u64, u64) acquires Pool {
        let share = add_liquidity(coin1_in, coin2_in);
        remove_liquidity(share)
    }
    spec no_free_money_theorem {
        pragma verify=false;
        ensures result_1 <= coin1_in;
        ensures result_2 <= coin2_in;
    }


    public fun coin1_to_coin2_swap_code_simple(coin1_in: u64): u64 acquires Pool {
        assert! (coin1_in > 0, 1);
        let pool = borrow_global_mut<Pool>(PoolAddr);
        spec {
            assume pool.coin1 > 0;
            assume pool.coin2 > 0;
        };
        let coin2_out = (997 * coin1_in * pool.coin2) / (1000 * pool.coin1 + 997 * coin1_in);
        pool.coin1 = pool.coin1 + coin1_in;
        pool.coin2 = pool.coin2 - coin2_out;
        coin2_out
    }
}
