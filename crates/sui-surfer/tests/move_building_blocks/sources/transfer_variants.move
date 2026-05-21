// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises the `public_*` transfer variants (the existing `objects` module
/// only uses the non-public `transfer`/`share_object`/`freeze_object`).
module move_building_blocks::transfer_variants {
    public struct Widget has key, store {
        id: UID,
        value: u64,
    }

    public fun create_owned(value: u64, ctx: &mut TxContext) {
        transfer::public_transfer(Widget { id: object::new(ctx), value }, ctx.sender());
    }

    public fun create_shared(value: u64, ctx: &mut TxContext) {
        transfer::public_share_object(Widget { id: object::new(ctx), value });
    }

    public fun create_frozen(value: u64, ctx: &mut TxContext) {
        transfer::public_freeze_object(Widget { id: object::new(ctx), value });
    }

    public fun transfer_to(widget: Widget, recipient: address) {
        transfer::public_transfer(widget, recipient);
    }

    public fun freeze_widget(widget: Widget) {
        transfer::public_freeze_object(widget);
    }
}
