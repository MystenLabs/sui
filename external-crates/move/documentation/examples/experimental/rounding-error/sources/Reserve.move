module NamedAddr::Reserve {
    use std::fixed_point32::{Self, FixedPoint32};

    const ADMIN: address = @NamedAddr;

    struct ReserveComponent has store {
        backing_value: u64,
        backing_ratio: FixedPoint32,
    }

    struct Coin1Info has key {
        total_value: u64,
        reserve_coin2: ReserveComponent,
    }

    public fun mint_coin1(amount_to_mint: u64, backing_coin2: u64): u64 // returns the minted Coin1.
    acquires Coin1Info {
        assert!(amount_to_mint > 0, 1);
        let coin1info = borrow_global_mut<Coin1Info>(ADMIN);
        let coin2_amount_to_reserve = fixed_point32::multiply_u64(amount_to_mint, *& coin1info.reserve_coin2.backing_ratio) + 1;
        assert!(backing_coin2 == coin2_amount_to_reserve, 2);
        coin1info.reserve_coin2.backing_value = coin1info.reserve_coin2.backing_value + coin2_amount_to_reserve;
        coin1info.total_value = coin1info.total_value + amount_to_mint;
        amount_to_mint
    }
    spec mint_coin1 {
        let coin1info = global<Coin1Info>(ADMIN);
        let coin2_amount_to_reserve = fixed_point32::spec_multiply_u64(amount_to_mint, coin1info.reserve_coin2.backing_ratio) + 1;
        aborts_if amount_to_mint == 0;
        aborts_if backing_coin2 != coin2_amount_to_reserve;
        aborts_if global<Coin1Info>(ADMIN).total_value + amount_to_mint > MAX_U64;
        aborts_if coin1info.reserve_coin2.backing_value + backing_coin2 > MAX_U64;
        aborts_if !exists<Coin1Info>(ADMIN);

        ensures global<Coin1Info>(ADMIN).total_value == old(global<Coin1Info>(ADMIN).total_value) + amount_to_mint;
        ensures global<Coin1Info>(ADMIN).reserve_coin2.backing_value == old(global<Coin1Info>(ADMIN).reserve_coin2.backing_value) + backing_coin2;
        ensures backing_coin2 == coin2_amount_to_reserve;
    }

    public fun mint_coin1_incorrect(amount_to_mint: u64, backing_coin2: u64): u64 // returns the minted Coin1.
    acquires Coin1Info {
        assert!(amount_to_mint > 0, 1);
        let coin1info = borrow_global_mut<Coin1Info>(ADMIN);
        let coin2_amount_to_reserve = fixed_point32::multiply_u64(amount_to_mint, *& coin1info.reserve_coin2.backing_ratio);
        assert!(backing_coin2 == coin2_amount_to_reserve, 2);
        coin1info.reserve_coin2.backing_value = coin1info.reserve_coin2.backing_value + coin2_amount_to_reserve;
        coin1info.total_value = coin1info.total_value + amount_to_mint;
        amount_to_mint
    }

    public fun burn_coin1(amount_to_burn: u64): u64 // returns the Coin2 that was reserved.
    acquires Coin1Info {
        let coin1info = borrow_global_mut<Coin1Info>(ADMIN);
        let coin2_amount_to_return = fixed_point32::multiply_u64(amount_to_burn, *& coin1info.reserve_coin2.backing_ratio);
        assert!(coin1info.reserve_coin2.backing_value >= coin2_amount_to_return, 1);
        coin1info.reserve_coin2.backing_value = coin1info.reserve_coin2.backing_value - coin2_amount_to_return;
        coin1info.total_value = coin1info.total_value - amount_to_burn;
        coin2_amount_to_return
    }

    public fun burn_coin1_incorrect(amount_to_burn: u64): u64 // returns the Coin2 that was reserved.
    acquires Coin1Info {
        let coin1info = borrow_global_mut<Coin1Info>(ADMIN);
        let coin2_amount_to_return = fixed_point32::multiply_u64(amount_to_burn, *& coin1info.reserve_coin2.backing_ratio) + 1;
        assert!(coin1info.reserve_coin2.backing_value >= coin2_amount_to_return, 1);
        coin1info.reserve_coin2.backing_value = coin1info.reserve_coin2.backing_value - coin2_amount_to_return;
        coin1info.total_value = coin1info.total_value - amount_to_burn;
        coin2_amount_to_return
    }

    fun mint_and_burn(amount_to_mint: u64, backing_coin2: u64): u64 acquires Coin1Info{
        let coin1 = mint_coin1(amount_to_mint, backing_coin2);
        let coin2 = burn_coin1(coin1);
        spec {
            assert coin2 <= backing_coin2;
        };
        coin2
    }

    spec module {
        invariant exists<Coin1Info>(ADMIN) ==>
            global<Coin1Info>(ADMIN).reserve_coin2.backing_ratio == fixed_point32::spec_create_from_rational(1, 2);

        invariant
            exists<Coin1Info>(ADMIN) ==>
            fixed_point32::spec_multiply_u64(global<Coin1Info>(ADMIN).total_value, global<Coin1Info>(ADMIN).reserve_coin2.backing_ratio)
                <= global<Coin1Info>(ADMIN).reserve_coin2.backing_value;
    }
}
