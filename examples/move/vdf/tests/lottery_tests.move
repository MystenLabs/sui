// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vdf::lottery_tests {
    use sui::test_scenario as ts;
    use sui::clock;
    use vdf::lottery::{Self, Game, GameWinner};

    const OUTPUT: vector<u8> =
        x"c0014d00b5e624fe10d1cc1e593c0ffb8c3084e49bb70efc4337640a73990bb29dfb430b55710475bcc7524c77627d8067415fffa63e0e84b1204225520fea384999719c66dbdc6e91863d99c64674af971631b56e22b7cc780765bf12d53edea1dadf566f80a62769e287a1e195596d4894b2e1360e451cbf06864762275b4d5063871d45627dea2e42ab93d5345bf172b9724216627abbf295b35a8e64e13e585bca54848a90212c9f7a3adffc25c3b87eefa7d4ab1660b523bf6410b9a9ea0e00c001327d73bebc768d150beb2a1b0de9e80c69ed594ae7787d548af44eb1e0a03616100133146c9c1202ea3a35c331864f3bfe59ffa3c88d6acb8af7a4b1b5ea842c4c4c88d415539389e614876d738d80217a3ad16d001f2de60f62b04a9d8de7ccb4716c3368f0d42e3e719dbb07bdb4355f0e5569714fbcc130f30ac7b49a5b207a444c7e00a0c27edae10c28b05f87545f337283f90c4e4ed69639683154d6a89e6589db4d18702d3a29705b434dc32e10fcbd3c62d1da20b45dba511bcecdf7c101009db7911c39f3937785dd8bd0e9db56c94777bdd867897493f82e0931e0e5facb730c3aa400f815f3d2f61de94373209dcbf9210c5e1f179b675adce2be7159238cb5f89c568004ccfc75d1b3de99a060255bcd6c9dd9a674844251ec27c48119b870f4ce63cac2081541c4269bbfa60148f9dbf2c60c76099a6538a33e88c24ce092e6aad9bdfff330470ffb87b01b666dd98e44843b41896f2be688361fe062692b441b4dd8f8ecfa96948d13daf486259bb8934cab7e9d9788da0eac7edf56";

    const PROOF: vector<u8> =
        x"c0010f1ea16b16f3fc46eb0b00a516092a6ab389e259a3013eee5e39e130702de85954b8aac435e724ad0bfd210ab7789fb91b54ac4352154f3251e0a87ccfd2c9a57d26468a384f527e129fc82b33c04b3ebbec3a99967798a95b39820c35ea015fdf4c81e143004b34b99e63462cf350689b2abdd6c3903adcbe55781d3a89c89dc571c312f9a80911a9d64884747319574b3a4ded25478e6d64b9cfb25d9c67366bc25d9ac99bcdba16665158da50a2ba179893292c4b7e76502ecaba1337d693c001fb3867669e0d4e45aa43d959dbe33c3d35b00e8414d1cf1bb9552726bb95bafa0a2c12a014a3b8fb0bd5ab9a40430ff59364b19d58d80665fee0bfee272a38c45413a3688832bf9bcacf7b5723436c120878f85ce084e72b13246ecfec7cd6a5d79e13296bbb51af785c10afe6c4f07f43a5bc711dc398271185d700b1695310d8e428ad3bc6b81a4faac2f5009b723460dbd260c940dfac06e34854d204dc779f94ab3f67847a7b90855dadc3962871c022e172e96b39a08648e045e14dad87c10102f976797f14be801a441f19771a4835640a74cf7c6ad216f18d9cdaf461bb56a897b804e053cd6cc68d659bd9f0ed985f094932d306c1bd76450bd349db3a81008d7591bc826a36583c3c361add7a8f245d18007d79704d79ae27eb08b52a44af17e2f23b441919049f061d69bac3a09c3e15074e4d75cf82f42dbff1c62ddc94fe6167ccb7265e7eab0def7d30d97be441ad763705dd30d4040815996e34643bf6d7a4f06c22aa5d6d5dd30253ea8aa59607724bb71f5425a5e7fee03b7e9fe8";

    const BAD_PROOF: vector<u8> =
        x"0101010180032cf35709e1301d02b40a0dbe3dadfe6ec1eeba8fb8060a1decd0c7a126ea3f27fadcad81435601b0e0abca5c89173ef639e5a88043aa29801e6799e430b509e479b57af981f9ddd48d3a8d5919f99258081557a08270bb441233c78030a01e03ec199b5e3eef5ccc9b1a3d4841cbe4ff529c22a8cd1b1b0075338d864e3890942df6b007d2c3e3a8ef1ce7490c6bbec5372adfcbf8704a1ffc9a69db8d9cdc54762f019036e450e457325eef74b794f3f16ff327d68079a5b9de49163d7323937374f8a785a8f9afe84d6a71b336e4de00f239ee3af1d7604a3985e610e1603bd0e1a4998e19fa0c8920ffd8d61b0a87eeee50ac7c03ff7c4708a34f3bc92fd0103758c954ee34032cee2c78ad8cdc79a35dbc810196b7bf6833e1c45c83b09c0d1b78bc6f8753e10770e7045b08d50b4aa16a75b27a096d5ec1331f1fd0a44e95a8737c20240c90307b5497d3470393c2a00da0649e86d13e820591296c644fc1eef9e7c6ca4967c5e19df3153cd7fbd598c271e11c10397349ddc8cc8452ec";

    #[test]
    #[expected_failure(abort_code = vdf::lottery::ESubmissionPhaseInProgress)]
    fun test_complete_too_early() {
        let user1 = @0x0;

        let mut ts = ts::begin(user1);
        let mut clock = clock::create_for_testing(ts.ctx());

        lottery::create(1000, 1000, &clock, ts.ctx());
        ts.next_tx(user1);
        let mut game: Game = ts.take_shared();

        // User 1 buys a ticket.
        ts.next_tx(user1);
        let _t1 = game.participate(b"user1 randomness", &clock, ts.ctx());

        // Increment time but still in submission phase
        clock.increment_for_testing(500);

        // User1 tries to complete the lottery too early.
        ts.next_tx(user1);
        game.complete(OUTPUT, PROOF, &clock);
        abort 0
    }

    #[test]
    fun test_play_vdf_lottery() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let mut ts = ts::begin(user1);
        let mut clock = clock::create_for_testing(ts.ctx());

        lottery::create(1000, 1000, &clock, ts.ctx());
        ts.next_tx(user1);
        let mut game: Game = ts.take_shared();

        // User 1 buys a ticket.
        ts.next_tx(user1);
        let t1 = game.participate(b"user1 randomness", &clock, ts.ctx());

        // User 2 buys a ticket.
        ts.next_tx(user2);
        let t2 = game.participate(b"user2 randomness", &clock, ts.ctx());

        // User 3 buys a ticket
        ts.next_tx(user3);
        let t3 = game.participate(b"user3 randomness", &clock, ts.ctx());

        // User 4 buys a ticket
        ts.next_tx(user4);
        let t4 = game.participate(b"user4 randomness", &clock, ts.ctx());

        // Increment time to after submission phase has ended
        clock.increment_for_testing(1000);

        // User 3 completes by submitting output and proof of the VDF
        ts.next_tx(user3);
        game.complete(OUTPUT, PROOF, &clock);

        // User 1 is the winner since the mod of the hash results in 0.
        ts.next_tx(user1);
        assert!(!ts::has_most_recent_for_address<GameWinner>(user1), 1);
        let winner = game.redeem(&t1, ts.ctx());

        // Make sure User1 now has a winner ticket for the right game id.
        ts.next_tx(user1);
        assert!(winner.game_id() == t1.game_id(), 1);

        t1.delete();
        t2.delete();
        t3.delete();
        t4.delete();
        winner.delete();

        clock.destroy_for_testing();
        ts::return_shared(game);
        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = vdf::lottery::EInvalidVdfProof)]
    fun test_invalid_vdf_output() {
        let user1 = @0x0;
        let user2 = @0x1;
        let user3 = @0x2;
        let user4 = @0x3;

        let mut ts = ts::begin(user1);
        let mut clock = clock::create_for_testing(ts.ctx());

        lottery::create(1000, 1000, &clock, ts.ctx());
        ts.next_tx(user1);
        let mut game: Game = ts.take_shared();

        // User1 buys a ticket.
        ts.next_tx(user1);
        let _t1 = game.participate(b"user1 randomness", &clock, ts.ctx());
        // User2 buys a ticket.
        ts.next_tx(user2);
        let _t2 = game.participate(b"user2 randomness", &clock, ts.ctx());
        // User3 buys a ticket
        ts.next_tx(user3);
        let _t3 = game.participate(b"user3 randomness", &clock, ts.ctx());
        // User4 buys a ticket
        ts.next_tx(user4);
        let _t4 = game.participate(b"user4 randomness", &clock, ts.ctx());

        // Increment time to after submission phase has ended
        clock.increment_for_testing(1000);

        // User3 completes by submitting output and proof of the VDF
        ts.next_tx(user3);
        game.complete(OUTPUT, BAD_PROOF, &clock);
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = vdf::lottery::EInvalidRandomness)]
    fun test_empty_randomness() {
        let user1 = @0x0;

        let mut ts = ts::begin(user1);
        let clock = clock::create_for_testing(ts.ctx());

        lottery::create(1000, 1000, &clock, ts.ctx());
        ts.next_tx(user1);
        let mut game: Game = ts.take_shared();

        // User1 buys a ticket, but with wrong randomness length.
        ts.next_tx(user1);
        let _t = game.participate(b"abcd", &clock, ts.ctx());
        abort 0
    }
}
