// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// locked coins cannot be transferred in any way

//# init --addresses test=0x0

//# publish
module test::m {
    use sui::locked_coin::LockedCoin;
    use sui::transfer;

    struct TestCoin { }

    fun t(coin: LockedCoin<TestCoin>) {
        transfer::transfer(coin, @0x42);
    }
}

//# publish
module test::m {
    use sui::locked_coin::LockedCoin;
    use sui::transfer;

    struct TestCoin { }

    fun t(coin: LockedCoin<TestCoin>) {
        transfer::share_object(coin);
    }
}

//# publish
module test::m {
    use sui::locked_coin::LockedCoin;
    use sui::transfer;

    struct TestCoin { }

    fun t(coin: LockedCoin<TestCoin>) {
        transfer::freeze_object(coin);
    }
}
