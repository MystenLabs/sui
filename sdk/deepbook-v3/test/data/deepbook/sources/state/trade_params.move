// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// TradeParams module contains the trade parameters for a trading pair.
module deepbook::trade_params {
    // === Structs ===
    public struct TradeParams has store, drop, copy {
        taker_fee: u64,
        maker_fee: u64,
        stake_required: u64,
    }

    // === Public-Package Functions ===
    public(package) fun new(taker_fee: u64, maker_fee: u64, stake_required: u64): TradeParams {
        TradeParams { taker_fee, maker_fee, stake_required }
    }

    public(package) fun maker_fee(trade_params: &TradeParams): u64 {
        trade_params.maker_fee
    }

    public(package) fun taker_fee(trade_params: &TradeParams): u64 {
        trade_params.taker_fee
    }

    /// Returns the taker fee for a user based on the active stake and volume in deep.
    /// Taker fee is halved if user has enough stake and volume.
    public(package) fun taker_fee_for_user(
        self: &TradeParams,
        active_stake: u64,
        volume_in_deep: u64,
    ): u64 {
        if (active_stake >= self.stake_required && volume_in_deep >= self.stake_required) {
            self.taker_fee / 2
        } else {
            self.taker_fee
        }
    }

    public(package) fun stake_required(trade_params: &TradeParams): u64 {
        trade_params.stake_required
    }
}
