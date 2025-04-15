module prettier::tree_sitter {
    public (package) fun function() {}
    public(package) fun function() {}


    fun macro_fail() {
        core::create_deleverage_ticket!(|
            mut a,
            pool: &mut cetus_pool::Pool<X,Y>,
            lp_position: &mut CetusPosition,
            delta_l: u128| say::hello()
        )
    }

    fun from_index(index: u64): Colour {
        match (index) {
            0 | 2 => Colour::Empty,
            1 => Colour::Black,
            2 => Colour::White,
            _ => abort 0,
        }
    }

    fun clamm() {
        // Empties the pool
        state.d = state.d - state.d * lp_coin_amount.to_u256() / total_supply.to_u256();

        let (coin_a, coin_b, coin_c) = (
            withdraw_coin<CoinA, LpCoin>(state, lp_coin_amount, min_amounts[0], total_supply, ctx),
            withdraw_coin<CoinB, LpCoin>(state, lp_coin_amount, min_amounts[1], total_supply, ctx),
            withdraw_coin<CoinC, LpCoin>(state, lp_coin_amount, min_amounts[2], total_supply, ctx),
        );
    }
}
