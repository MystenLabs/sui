// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Tutorial::Ch1 {
    use Sui::ID::VersionedID;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    struct ColorObject has key {
        id: VersionedID,
        red: u8,
        green: u8,
        blue: u8,
    }

    fun new_color_object(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
        ColorObject {
            id: TxContext::new_id(ctx),
            red,
            green,
            blue,
        }
    }

    public fun create_color_object(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
        let color_object = new_color_object(red, green, blue, ctx);
        Transfer::transfer(color_object, TxContext::sender(ctx))
    }
}
