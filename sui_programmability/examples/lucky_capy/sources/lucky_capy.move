// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module lucky_capy::lucky_capy {

    use sui::object::{Self, ID, UID};
    use std::string;
    use sui::event;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;
    use lucky_capy::pseudorandom;

    // Errors
    const ENonInitialized: u64 = 0;
    const ENonRunning: u64 = 1;
    const EEnded: u64 = 2;

    struct LuckyCapy has key, store {
        id: UID,
        /// Name for the capy
        name: string::String,
        /// Score for the capy 
        score: u128,
    }

    struct LuckyCapyLottery has key {
        id: UID,
        // Capy participants
        capys: vector<LuckyCapy>,
        // Lottery status
        status: u8,
        round: u8,
    }

    // Lottery status
    const INITIALIZED: u8 = 0;
    const RUNNING: u8 = 1;
    const ENDED: u8 = 2;

    struct CapyBoostEvent has copy, drop {
        lottery_id: ID,
        round: u8,
        boost_value: u64,
        capy_name: string::String,
        capy_no: u64,
        new_score: u128,
        boost_value_combined: u128,
        capy_roller: string::String,
    }

    struct LotteryCreatedEvent has copy, drop {
        lottery_id: ID,
    }

    struct LotteryEndedEvent has copy, drop {
        lottery_id: ID,
        round: u8,
    }

    public entry fun create_lottery(
        ctx: &mut TxContext
    ) {
        let lottery = LuckyCapyLottery {
            id: object::new(ctx),
            capys: vector::empty(),
            status: INITIALIZED,
            round: 0,
        };
        event::emit(LotteryCreatedEvent {
            lottery_id: object::uid_to_inner(&lottery.id),
        });
        let sender = tx_context::sender(ctx);
        transfer::transfer(lottery, sender);
    }

    public entry fun add_capy(
        lottery: &mut LuckyCapyLottery,
        name: vector<u8>,
        ctx: &mut TxContext
    ) {
        // TODO check the name is not there yet?
        assert!(lottery.status == INITIALIZED, ENonInitialized);
        let name_str = string::utf8(name);
        let capy = LuckyCapy {
            id: object::new(ctx),
            name: name_str,
            score: 0,
        };
        vector::push_back(&mut lottery.capys, capy);
    }


    public entry fun roll_dice(
        lottery: &mut LuckyCapyLottery,
        capy_dice_roller: vector<u8>,
        seed: vector<u8>,
        ctx: &mut TxContext
    ) {
        assert!(lottery.status != ENDED, EEnded);
        lottery.status = RUNNING;
        let n = vector::length(&lottery.capys);

        // Decide the number of boost for this round
        let num_of_boosts = pseudorandom::rand_u64_range(ctx, capy_dice_roller, n/2, n+1);
        let i = 0;
        let base: u64 = 1 << lottery.round;

        while (i < num_of_boosts) {
            // Add some randomization for each boost
            let id = object::new(ctx);
            vector::append(&mut seed, object::uid_to_bytes(&id));
            object::delete(id);

            let boost_value = pseudorandom::rand_u64_range(ctx, seed, 1, 16);
            let jitter: u64 = pseudorandom::rand_u64_range(ctx, seed, 0, 10);
            let boost_value_combined = boost_value * base + jitter;

            let capy_no = pseudorandom::rand_u64_range(ctx, seed, 0, n);
            let capy = vector::borrow_mut(&mut lottery.capys, capy_no);
            capy.score = capy.score + (boost_value_combined as u128);
            event::emit(CapyBoostEvent {
                lottery_id: object::uid_to_inner(&lottery.id),
                round: lottery.round,
                boost_value: boost_value,
                boost_value_combined: (boost_value_combined as u128), 
                capy_name: capy.name,
                capy_no: capy_no,
                new_score: capy.score,
                capy_roller: string::utf8(capy_dice_roller),
            });
            i = i + 1;
        };
        lottery.round = lottery.round + 1;
    }

    public entry fun end_lottery(
        lottery: &mut LuckyCapyLottery,
        _ctx: &mut TxContext
    ) {
        assert!(lottery.status == RUNNING, ENonRunning);
        lottery.status = ENDED;
        event::emit(LotteryEndedEvent {
            lottery_id: object::uid_to_inner(&lottery.id),
            round: lottery.round,
        });
    }
}

#[test_only]
module lucky_capy::lucky_capyTests {
    use lucky_capy::lucky_capy::{Self, LuckyCapyLottery};
    use sui::test_scenario;

    #[test]
    fun test_ok() {
        let addr1 = @0xA;
        
        let scenario = test_scenario::begin(&addr1);
        lucky_capy::create_lottery(test_scenario::ctx(&mut scenario));

        let i = 0;
        while (i < 20) {
            test_scenario::next_tx(&mut scenario, &addr1);
            let lottery = test_scenario::take_owned<LuckyCapyLottery>(&mut scenario);
            lucky_capy::add_capy(&mut lottery, b"lu", test_scenario::ctx(&mut scenario));
            test_scenario::return_owned(&mut scenario, lottery);
            i = i + 1;
        };
        let i = 0;
        while (i < 10) {
            test_scenario::next_tx(&mut scenario, &addr1);
            let lottery = test_scenario::take_owned<LuckyCapyLottery>(&mut scenario);
            lucky_capy::roll_dice(&mut lottery, b"kostas", b"haha", test_scenario::ctx(&mut scenario));
            test_scenario::return_owned(&mut scenario, lottery);
            i = i + 1;
        };
        test_scenario::next_tx(&mut scenario, &addr1);
        let lottery = test_scenario::take_owned<LuckyCapyLottery>(&mut scenario);
        lucky_capy::end_lottery(&mut lottery, test_scenario::ctx(&mut scenario));
        test_scenario::return_owned(&mut scenario, lottery);
    }
}
