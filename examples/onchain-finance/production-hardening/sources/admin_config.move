// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#admin_config
module example::admin_config;

const EPaused: u64 = 0;
const ENotAdmin: u64 = 1;

public struct AdminConfig has key {
    id: UID,
    paused: bool,
    admin: address,
}

public struct AdminCap has key, store {
    id: UID,
}

fun init(ctx: &mut TxContext) {
    let config = AdminConfig {
        id: object::new(ctx),
        paused: false,
        admin: ctx.sender(),
    };
    transfer::share_object(config);
    transfer::transfer(AdminCap { id: object::new(ctx) }, ctx.sender());
}

public fun assert_not_paused(config: &AdminConfig) {
    assert!(!config.paused, EPaused);
}

public fun pause(_cap: &AdminCap, config: &mut AdminConfig) {
    config.paused = true;
}

public fun unpause(_cap: &AdminCap, config: &mut AdminConfig) {
    config.paused = false;
}
// docs::/#admin_config
