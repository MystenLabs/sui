// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//docs::#regulate
module examples::regcoin {
    use sui::coin::{Self, DenyCap};
    use sui::deny_list::{DenyList};

    public struct REGCOIN has drop {}

    fun init(witness: REGCOIN, ctx: &mut TxContext) {
        let (treasury, deny_cap, metadata) = coin::create_regulated_currency(witness, 6, b"REGCOIN", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());
        transfer::public_transfer(deny_cap, ctx.sender())
    }

    //docs::/#regulate}

    public fun add_addr_from_deny_list(denylist: &mut DenyList, denycap: &mut DenyCap<REGCOIN>, denyaddy: address, ctx: &mut TxContext) {
        coin::deny_list_add(denylist, denycap, denyaddy, ctx);
    }

    public fun remove_addr_from_deny_list(denylist: &mut DenyList, denycap: &mut DenyCap<REGCOIN>, denyaddy: address, ctx: &mut TxContext){
        coin::deny_list_remove(denylist, denycap, denyaddy, ctx);
    }
}