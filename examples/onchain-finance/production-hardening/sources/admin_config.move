// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::admin_config;

#[error]
const EPaused: vector<u8> = b"System is paused";

// docs::#admin_config
public struct AdminConfig has key {
    id: UID,
    is_paused: bool,
}

public struct AdminCap has key, store {
    id: UID,
    config_id: ID,
}

fun init(ctx: &mut TxContext) {
    let config = AdminConfig {
        id: object::new(ctx),
        is_paused: false,
    };
    let cap = AdminCap {
        id: object::new(ctx),
        config_id: config.id.to_inner(),
    };
    transfer::share_object(config);
    transfer::public_transfer(cap, ctx.sender());
}

public fun assert_not_paused(config: &AdminConfig) {
    assert!(!config.is_paused, EPaused);
}

public fun pause(config: &mut AdminConfig, cap: &AdminCap) {
    assert!(cap.config_id == config.id.to_inner());
    config.is_paused = true;
}

public fun unpause(config: &mut AdminConfig, cap: &AdminCap) {
    assert!(cap.config_id == config.id.to_inner());
    config.is_paused = false;
}
// docs::/#admin_config
