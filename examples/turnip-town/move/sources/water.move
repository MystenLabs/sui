// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Water is a finite resource -- every address is able to request a fixed
/// quantity per epoch, and can use it to water turnips. Water is fetched from
/// Wells and must be used in the same transaction it was fetched in.
module turnip_town::water {
    /// Water can be `drop`-ped, but it cannot be `store`d or transferred, to
    /// prevent stockpiling -- it must be used in the same transaction it was
    /// requested in.
    public struct Water has drop {
        balance: u64
    }

    /// Well is a store of Water that replenishes every epoch (but does not
    /// accumulate -- it reaches the same level at the beginning of every
    /// epoch).
    public struct Well has store, drop {
        last_used: u64,
        available: u64,
    }

    // === Constants ===

    const WATER_PER_EPOCH: u64 = 100;

    // === Errors ===

    /// Not enough water to satisfy request.
    const ENotEnough: u64 = 0;

    // === Public functions ===

    /// Get water from the well. Also replenishes the well for this epoch if
    /// that hasn't happened yet. Aborts if there is not enough water in the
    /// well to satisfy the request.
    public fun fetch(well: &mut Well, amount: u64, ctx: &TxContext): Water {
        let epoch = ctx.epoch();
        if (well.last_used < epoch) {
            well.available = WATER_PER_EPOCH;
            well.last_used = epoch;
        };

        assert!(amount <= well.available, ENotEnough);
        well.available = well.available - amount;

        Water { balance: amount }
    }

    public fun zero(): Water {
        Water { balance: 0 }
    }

    public fun split(self: &mut Water, balance: u64): Water {
        assert!(self.balance >= balance, ENotEnough);
        self.balance = self.balance - balance;
        Water { balance }
    }

    public fun join(self: &mut Water, water: Water): u64 {
        let Water { balance } = water;
        self.balance = self.balance + balance;
        self.balance
    }

    public fun value(self: &Water): u64 {
        self.balance
    }

    // === Protected functions ===

    /// Create a new, filled well.
    public(package) fun well(ctx: &TxContext): Well {
        Well {
            last_used: ctx.epoch(),
            available: WATER_PER_EPOCH,
        }
    }

    // === Test Helpers ===

    #[test_only]
    public fun per_epoch(): u64 {
        WATER_PER_EPOCH
    }

    #[test_only]
    public fun for_test(balance: u64): Water {
        Water { balance }
    }
}
