// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B

//# publish --sender A

module test::transfer_coin_to_account {
    use sui::coin::{Coin, from_balance};
    use sui::sui::SUI;
    use sui::balance::{MergableBalance, withdraw_from_account};
    use sui::account;
    use sui::transfer::public_transfer;

    public fun transfer(c: Coin<SUI>, address: address) {
        c.send_to_account(address);
    }

    // withdraw from an account and transfer coin to self
    public fun withdraw(mut reservation: account::Reservation<MergableBalance<SUI>>, amount: u64, ctx: &mut TxContext) {
        let coin = from_balance(withdraw_from_account<SUI>(&mut reservation, amount), ctx);
        public_transfer(coin, ctx.sender());
    }
}

//# programmable --sender A --inputs 10 @B
//> SplitCoins(Gas, [Input(0)]);
//> test::transfer_coin_to_account::transfer(Result(0), Input(1))

//# programmable --sender B --inputs 10 @B
//> sui::account::reserve<sui::balance::MergableBalance<sui::sui::SUI>>(Input(1), Input(0));
//> test::transfer_coin_to_account::withdraw(Result(0), Input(0))