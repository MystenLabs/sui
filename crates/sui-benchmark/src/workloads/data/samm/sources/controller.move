// Copyright 2022 OmniBTC Authors. Licensed under Apache-2.0 License.
module swap::controller {
    use sui::tx_context::{Self, TxContext};

    use swap::implements::{Self, Global};

    const ERR_NO_PERMISSIONS: u64 = 201;
    const ERR_ALREADY_PAUSE: u64 = 202;
    const ERR_NOT_PAUSE: u64 = 203;

    /// Entrypoint for the `pause` method.
    /// Pause all pools under the global.
    public entry fun pause(global: &mut Global, ctx: &mut TxContext) {
        assert!(!implements::is_emergency(global), ERR_ALREADY_PAUSE);
        assert!(implements::controller(global) == tx_context::sender(ctx), ERR_NO_PERMISSIONS);
        implements::pause(global)
    }

    /// Entrypoint for the `resume` method.
    /// Resume all pools under the global.
    public entry fun resume(global: &mut Global, ctx: &mut TxContext) {
        assert!(implements::is_emergency(global), ERR_NOT_PAUSE);
        assert!(implements::controller(global) == tx_context::sender(ctx), ERR_NO_PERMISSIONS);
        implements::resume(global)
    }

    /// Entrypoint for the `modify_controller` method.
    /// Set new controller
    public entry fun modify_controller(
        global: &mut Global, new_controller: address, ctx: &mut TxContext
    ) {
        assert!(implements::controller(global) == tx_context::sender(ctx), ERR_NO_PERMISSIONS);
        implements::modify_controller(global, new_controller)
    }
}
