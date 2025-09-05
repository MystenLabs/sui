// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// arbitrary struct input disallowed for dry run

//# init --addresses test=0x0 --accounts A

//# publish

module test::m {
    public struct S has key {
        id: UID,
        setting: u64,
    }

    public fun take_s(s: S) {
        let S { id, .. } = s;
        id.delete();
    }
}


//# programmable --sender A --inputs struct(@empty,0) --dry-run
//> 0: test::m::take_s(Input(0));
