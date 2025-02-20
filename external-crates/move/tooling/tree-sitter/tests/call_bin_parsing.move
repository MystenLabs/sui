// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module 0x1::a;
fun g() {
    let x = withdraw_coin<CoinA, LpCoin>(state, lp_coin_amount, min_amounts[0], total_supply, ctx);
    (withdraw_coin<CoinA, LpCoin>(state, lp_coin_amount, min_amounts[0], total_supply, ctx),
     withdraw_coin<CoinB, LpCoin>(state, lp_coin_amount, min_amounts[1], total_supply, ctx),
     withdraw_coin<CoinB, LpCoin>(
         state,
         lp_coin_amount,
         min_amounts[1],
         total_supply,
         withdraw_coin<CoinA, LpCoin>(state, lp_coin_amount, min_amounts[0], total_supply, ctx),
         (withdraw_coin<CoinA, LpCoin>(state, lp_coin_amount, min_amounts[0], total_supply, ctx)),
     ),
    );
}
