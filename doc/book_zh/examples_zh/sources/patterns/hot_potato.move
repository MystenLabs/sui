// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::trade_in {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext};

    /// 第一种手机的价格
    /// Price for the first phone model in series
    const MODEL_ONE_PRICE: u64 = 10000;

    /// 第二种手机的价格
    /// Price for the second phone model
    const MODEL_TWO_PRICE: u64 = 20000;

    /// 错误码 1：购买的手机类型不存在
    /// For when someone tries to purchase non-existing model
    const EWrongModel: u64 = 1;

    /// 错误码 2：支付金额不足
    /// For when paid amount does not match the price
    const EIncorrectAmount: u64 = 2;

    /// Phone: 可以被购买或者以旧换新
    /// A phone; can be purchased or traded in for a newer model
    struct Phone has key, store { id: UID, model: u8 }

    /// Receipt: 可以直接支付或者接受以旧换新，不可以被储存，拥有或者丢弃， 
    /// 必须在`trade_in` 或者 `pay_full` 方法中被消耗
    /// Payable receipt. Has to be paid directly or paid with a trade-in option.
    /// Cannot be stored, owned or dropped - has to be used to select one of the
    /// options for payment: `trade_in` or `pay_full`.
    struct Receipt { price: u64 }

    /// 购买手机，返回的`Receipt`必须在`trade_in` 或者 `pay_full`中被消耗。
    /// Get a phone, pay later.
    /// Receipt has to be passed into one of the functions that accept it:
    ///  in this case it's `pay_full` or `trade_in`.
    public fun buy_phone(model: u8, ctx: &mut TxContext): (Phone, Receipt) {
        assert!(model == 1 || model == 2, EWrongModel);

        let price = if (model == 1) MODEL_ONE_PRICE else MODEL_TWO_PRICE;

        (
            Phone { id: object::new(ctx), model },
            Receipt { price }
        )
    }

    /// 全款支付，获得`Phone`对象，同时`Receipt`被消耗
    /// Pay the full price for the phone and consume the `Receipt`.
    public fun pay_full(receipt: Receipt, payment: Coin<SUI>) {
        let Receipt { price } = receipt;
        assert!(coin::value(&payment) == price, EIncorrectAmount);

        // for simplicity's sake transfer directly to @examples account
        transfer::public_transfer(payment, @examples);
    }

    /// 以旧换新，传入一个已有的`Phone`对象，获得新的`Phone`对象，同时`Receipt`被消耗
    /// Give back an old phone and get 50% of its price as a discount for the new one.
    public fun trade_in(receipt: Receipt, old_phone: Phone, payment: Coin<SUI>) {
        let Receipt { price } = receipt;
        let tradein_price = if (old_phone.model == 1) MODEL_ONE_PRICE else MODEL_TWO_PRICE;
        let to_pay = price - (tradein_price / 2);

        assert!(coin::value(&payment) == to_pay, EIncorrectAmount);

        transfer::public_transfer(old_phone, @examples);
        transfer::public_transfer(payment, @examples);
    }
}
