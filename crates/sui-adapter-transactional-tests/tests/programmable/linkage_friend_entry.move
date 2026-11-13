// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Friend-entry and private targets pin dependency versions exactly, while public targets
// permit newer versions. Tests direct friend-entry calls, both conflicting command orders,
// dev-inspect private calls, and independent v1 selection after v2 has been cached.

//# init --addresses DepV1=0x0 DepV2=0x0 Root=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public fun check(expected: u64) { assert!(expected == 1, 0) }
}

//# publish --dependencies DepV1 --sender A
module Root::m {
    public(package) entry fun friend_entry(expected: u64) { DepV1::m::check(expected) }

    public fun public_call(expected: u64) { DepV1::m::check(expected) }

    fun private_call(expected: u64) { DepV1::m::check(expected) }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public fun check(expected: u64) { assert!(expected == 2, 0) }
}

//# programmable --sender A --inputs 1u64
// A direct friend-entry target pins its dependency exactly to v1.
//> Root::m::friend_entry(Input(0));

//# programmable --sender A --inputs 1u64 2u64
// Friend visibility must not be treated as public when folding dependencies.
//> Root::m::friend_entry(Input(0));
//> DepV2::m::check(Input(1));

//# programmable --sender A --inputs 1u64 2u64
// Reversing the conflicting commands still fails during preparation.
//> DepV2::m::check(Input(1));
//> Root::m::friend_entry(Input(0));

//# programmable --sender A --inputs 2u64
// The public control permits the same dependency upgrade.
//> Root::m::public_call(Input(0));
//> DepV2::m::check(Input(0));

//# programmable --sender A --inputs 1u64
// The warmed v2 package does not change a later friend-entry transaction's exact v1 pin.
//> Root::m::friend_entry(Input(0));

//# programmable --sender A --dev-inspect --inputs 1u64
// Dev-inspect permits private targets, but preserves their exact dependency policy.
//> Root::m::private_call(Input(0));

//# programmable --sender A --dev-inspect --inputs 1u64 2u64
// Bypassing entry visibility checks must not bypass exact dependency constraints.
//> Root::m::private_call(Input(0));
//> DepV2::m::check(Input(1));
