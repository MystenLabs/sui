// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// clock has only key ability

//# init --addresses test=0x0

//# publish
module test::m {
    use sui::clock::Clock;
    use sui::transfer;

    fun t(clock: Clock) {
        transfer::transfer(clock, @0x42);
    }
}

//# publish
module test::m {
    use sui::clock::Clock;
    use sui::transfer;

    fun t(clock: Clock) {
        transfer::share_object(clock);
    }
}

//# publish
module test::m {
    use sui::clock::Clock;
    use sui::transfer;

    fun t(clock: Clock) {
        transfer::freeze_object(clock);
    }
}
