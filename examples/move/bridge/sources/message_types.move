// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::message_types {
    // message types
    const TOKEN: u8 = 0;
    const COMMITTEE_BLOCKLIST: u8 = 1;
    const EMERGENCY_OP: u8 = 2;
    //const COMMITTEE_CHANGE: u8 = 2;
    //const NFT: u8 = 4;

    public fun token():u8{
        TOKEN
    }

    public fun committee_blocklist():u8{
        COMMITTEE_BLOCKLIST
    }

    public fun emergency_op():u8{
        EMERGENCY_OP
    }
}
