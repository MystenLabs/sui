// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::test1 {
    use sui::random::{Random, RandomGenerator};
    friend a::test2;

    entry fun should_work(_r: &Random) {}

    public fun not_allowed1(_x: u64, _r: &Random) {}
    public fun not_allowed2(_rg: &RandomGenerator, _x: u64) {}
    public fun not_allowed3(_r: &Random, _rg: &RandomGenerator, _x: u64) {}

    #[allow(lint(public_random))]
    public fun allow_public_random_should_work(_r: &Random, _rg: &RandomGenerator) {}

    public(friend) fun public_friend_should_work(_r: &Random, _rg: &RandomGenerator) {}

    fun private_should_work(_r: &Random, _rg: &RandomGenerator) {}
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::random {
    use sui::object::UID;

    struct Random has key { id: UID }
    struct RandomGenerator has drop {}

    public fun should_work(_r: &Random) {}
}

module a::test2 {

}
