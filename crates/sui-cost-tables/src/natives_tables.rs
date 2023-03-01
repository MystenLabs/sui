// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::gas_algebra::{GasQuantity, InternalGas, InternalGasUnit};

//
// Native function costs
//
// TODO: need to refactor native gas calculation so it is extensible. Currently we
// have hardcoded here the stdlib natives.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SuiNativeCostIndex {
    EVENT_EMIT = 0,

    OBJECT_BYTES_TO_ADDR = 1,
    OBJECT_BORROW_UUID = 2,
    OBJECT_DELETE_IMPL = 3,

    TRANSFER_TRANSFER_INTERNAL = 4,
    TRANSFER_FREEZE_OBJECT = 5,
    TRANSFER_SHARE_OBJECT = 6,

    TX_CONTEXT_DERIVE_ID = 7,
    TX_CONTEXT_NEW_SIGNER_FROM_ADDR = 8,
}

pub const NATIVES_COST_LOW: GasQuantity<InternalGasUnit> = InternalGas::new(10);
pub const NATIVES_COST_MID: GasQuantity<InternalGasUnit> = InternalGas::new(100);
pub const NATIVES_COST_HIGH: GasQuantity<InternalGasUnit> = InternalGas::new(10000);

/// Base fee for entering a native fn
pub const NATIVES_COST_BASE_ENTRY: GasQuantity<InternalGasUnit> = NATIVES_COST_HIGH;

#[test]
pub fn test_natives_cost_tiers() {
    assert!((NATIVES_COST_LOW < NATIVES_COST_MID) && (NATIVES_COST_MID < NATIVES_COST_HIGH));
}
