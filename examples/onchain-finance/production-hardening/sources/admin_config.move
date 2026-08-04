// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::admin_config;

#[error(code = 0)]
const EPaused: vector<u8> = b"System is paused";
#[error(code = 1)]
const EWrongAdminCap: vector<u8> = b"Capability does not match this configuration";

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
        config_id: object::id(&config),
    };
    transfer::share_object(config);
    transfer::public_transfer(cap, ctx.sender());
}

public fun assert_not_paused(config: &AdminConfig) {
    assert!(!config.is_paused, EPaused);
}

public fun pause(config: &mut AdminConfig, cap: &AdminCap) {
    assert!(cap.config_id == object::id(config), EWrongAdminCap);
    config.is_paused = true;
}

public fun unpause(config: &mut AdminConfig, cap: &AdminCap) {
    assert!(cap.config_id == object::id(config), EWrongAdminCap);
    config.is_paused = false;
}
// docs::/#admin_config
