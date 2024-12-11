// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vdf::lottery_tests;

use sui::{clock, test_scenario as ts};
use vdf::lottery::{Self, Game, GameWinner};

const OUTPUT: vector<u8> =
    x"c00139196755e0b52e791d3d6bc79cae46ddb8538d78b41a5e960d161748a144268da911c3cd252be589324289fa5f15cca87ce5f0721fad3ac4a63ba8e46512bd5d1deb0878b4de69a11b34f825434b468f461ae1463f969bd4ebcb14ad41bfd2a648fb70ff1ba30d6924b8a29dce3369c247890f2dfc5b636dbaf3e57f7245cc1e278aef91aa4fac88dde0e91874817e0b306116ce8e9e8fbe20ff1a0b991a5b395f4d26155eb975267506678bbe53d2b36b389cb4285441d6e9a2d65bc9c09aa6c001fdc198d0915db5073427cd42d0cf050a8f1b017211349f217240af8e65fc22e17684d88c7ce65f6f3bb4d2dbd0cb18905958e1e685608ab41b7bc43937354bab5ac29fe6247a0d5ee0c62e269103a9aca26c4685131f84de12faa2aec64782f8be17b1a9236614dec1c0c9622846246135322ed0d6967dd34cf30e1df61667c0026665080ed5939644f0fce70f375240b4f7dcc3fdbc27ad69f46041c922e260579376f3c0fe231cd28a466214e9593e410c47bf2c91aa6231158a35ea5c6cd9c10100c98de9aafb908426a12147801b4483a92c6194b882b725d2811728b03eb4451349434fba9c9882021765b9f5ec9fae4d469603291086ad32c6ed6b40b03f77a08d2ae784f7d0359b855018872307d760c4bfc731519f9859eef844fb61f60c67c5f6a091dbe40601f2bc8d9b029bee501c412d20e0c7dac29ca7f6942a46216cc11337245d9e5df5fe1c951f1da3c4f36a2dfa7348b8d399c6f116448e75adb1b8b98b83d08b8f03911586153ecb9c953259b244083f3625cd15381633a36c3c";

const PROOF: vector<u8> =
    x"c0014c3726e4a8b4748edf1e536168bd6f65b261a24777d41745badb3c475462a41f91b0fc1824682274ccd6b112aa29c0bdd4d314e718964d80903a89a22545a1a375be431ee0b42d4aa2948a624af88348386bb4cbc3c7a0e9e8e61bae5cb545eb0a85a85a872efb06fbfbb4241815cc157a2b98805744b3c49c9fa3b4dd465549503be15dc97ebb2fa0d50bccac77f80a8e37497481d96bdd4f93f0365a3fc832786b4c134772f11e42cf427e10bdd4cf1d8f15ed66b40e9f696dff7d39526f4cc001b67117474c19190057a21fe396239d6d57901e225e7396e743316f0104ffdfb0391de5fc2af4c5da63073cf6315e741e5ce7f5abcd1e8dfbd75e26bf477c41ddddb225d431ee7236b0e086fb521140eb708db3d6fd908140604b308a451b4967f9dc14a0a679152ee9c2d49ef5441b3d452bf44215e97a7ec6887b03db19bbb2a465216dcd6ee978a5007ea61c208c341659568b079fbecb7fd1616d90f48ff13fef190e847f5d274c7e96f989cce6d66cfc6795cdd7ab5b186504f9ad1a3867c10100a8bb98567e45419001c914099e225a8f821f69d42b3f6ba5fa0795df4a16150bce84f47da24ae32097ff6a72b0945276d879a0f91663f33e4e8a4cebb6edfa32991f6f6838ad5e048d40fccf67ab6477b5f144d8552d20427c635cbf8385d1213666cc4fa574c691c1d86cc9bbf7d4027daa480039c6840de49f9cb3f044f4c0f08bf16ef5c75b9c0ec17638b83726ff0a0dfdf471461aabf18d9d424bf17ddc4b79e5c837f7e1ed06454de8d366fd6c99402ea00c184eee44d282b622d8fd26";

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

    // User 2 is the winner since the mod of the hash results in 1.
    ts.next_tx(user2);
    assert!(!ts::has_most_recent_for_address<GameWinner>(user2), 1);
    let winner = game.redeem(&t2, ts.ctx());

    // Make sure User2 now has a winner ticket for the right game id.
    ts.next_tx(user2);
    assert!(winner.game_id() == t2.game_id(), 1);

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
